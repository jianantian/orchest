# 006 · Multi-subscriber Events + Watcher + InjectCmd

## 背景

v0.7 已在 `AgentRunState` 内部预埋了 `event_subs: Vec<mpsc::Sender<RuntimeEvent>>` 和 `AgentMsg::{ Inject(InjectCmd, _), Steer(SteerCmd, _) }` forward declaration，但：

1. **订阅 API 未暴露**：`RunHandle` 没有 `subscribe_events()` 方法，外部代码只能用 `AgentRun::start` 返回的单一 `EventReceiver`
2. **InjectCmd 是空结构体**：无法携带实际消息内容，handler 返回"not yet implemented"
3. **Watcher trait 未定义**：没有注册 watcher 的 API

本 issue 是 v0.8 中 Supervised Delegation 的**通信层**基础，v0.9 的 Mid-run Steering 在此之上暴露用户 API。

本 issue 独立于 001-004，可并行开发；003（ApprovalMode）不冲突。

## 目标

1. 暴露多订阅者 API：`RunHandle::subscribe_events(capacity)`
2. `InjectCmd` 补充 `message` payload，接通 `Inject` → actor → loop 链路
3. 定义 `Watcher` trait 和 `RunHandle::attach_watcher()`

## 范围

### AgentMsg::Subscribe — 新增变体

```rust
// run/actor.rs（pub(crate) enum AgentMsg）

AgentMsg::Subscribe(mpsc::Sender<RuntimeEvent>),
```

Actor `handle()` 中处理 `Subscribe`：将 tx 追加到 `state.event_subs`。注意 Ractor 的 `handle()` 签名为 `async fn handle(&self, myself, msg, state: &mut AgentRunState)`——state 是**参数**，直接用，不是 `self.state`：

```rust
AgentMsg::Subscribe(tx) => {
    state.event_subs.push(tx);
    Ok(())
}
```

### RunHandle::subscribe_events

```rust
// run/handle.rs

impl RunHandle {
    pub async fn subscribe_events(&self, capacity: usize) -> crate::run::EventReceiver {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Subscribe(tx));
            }
        }
        rx
    }
}
```

**投递保证（文档）**：次级订阅者是有损的——channel 满时 emit() 用 try_send，满时丢弃并向主订阅者发出 `RuntimeEvent::EventsDropped { subscriber_id, count }`。调用方应选择足够大的 `capacity`（建议 >= 1024）。

#### actor_ref 就绪时序（必须处理）

`AgentRun::start` 内部 `actor_ref` 初值为 `None`，在 spawn 的后台任务里 `Actor::spawn().await` 完成后才被设置（`run/mod.rs:69-77`）。这带来两个时序问题：

1. **订阅可能静默丢失**：紧跟 `start()` 调用 `subscribe_events()` 时 `actor_ref` 可能仍是 `None`，`if let Some(aref)` 直接跳过，订阅丢失。`RunHandle::abort()` 有同样的既有竞态，但对 abort 影响小；对 watcher 是致命的（核心功能不可靠）。
2. **错过订阅前事件**：`pre_start` 在 `Actor::spawn` 期间就 emit 了 `RunStarted`，即 actor_ref 就绪时 `RunStarted` 已发出。次级订阅者**必然错过 RunStarted 及订阅前的事件**——这与"次级订阅有损"的契约一致，但意味着 watcher / 测试不应依赖收到 `RunStarted`。

**解决（针对问题 1）**：`subscribe_events` 在 cast 前 await actor_ref 就绪。`start_with_bus` 增加就绪信号（`Arc<tokio::sync::Notify>` 或 `watch<bool>`），在 `*guard = Some(aref)` 后 `notify_waiters()`；`subscribe_events` 循环 `while actor_ref 为 None { 等待就绪信号 }` 再 cast。这样 `attach_watcher` 紧随 `start()` 也能可靠订阅（仍可能错过 RunStarted，属问题 2 的预期语义）。

> 问题 2 不修（符合有损契约）。Supervised Delegation 的 watcher 关注的是运行中的事件流，错过 RunStarted 可接受。

### InjectCmd — 补充 payload，改为单向 cast

v0.7 中 `Inject` 带 `RpcReplyPort<()>`（call 语义）。v0.8 **改为单向 `cast`**（无 reply）：注入是 fire-and-forget 软干预，不需要回复确认；watcher task 也不应阻塞等待 actor 回复。背压控制（若 v0.9 需要）届时再升级为 call。

```rust
// run/actor.rs

pub(crate) struct InjectCmd {
    pub message: String,
}

// AgentMsg 中 Inject 去掉 RpcReplyPort：
//   Inject(InjectCmd),          // v0.8：单向
//   Steer(SteerCmd, RpcReplyPort<SteerResult>),  // 保留 call 语义（v0.9 实现）
```

Actor `handle()` 中处理 `Inject`（替换现有"not yet implemented" stub；`state` 同为 handle 参数）：

```rust
AgentMsg::Inject(cmd) => {
    // 将消息插入 messages，以 User role 注入，下一个 RunStep 时模型可见
    state.messages.push(crate::model::Message {
        role: crate::model::Role::User,
        content: vec![crate::model::ContentBlock::Text { text: cmd.message }],
    });
    Ok(())
}
```

**Inject 的时序**：actor mailbox 是 FIFO，`Inject` 消息在队列中排队；实际插入时机取决于当时的 mailbox 状态。通常情况下，`Inject` 在下一个 `RunStep` 开始前被处理（RunStep 消息由上一个 RunStep 结束时 cast，Inject 排在其后）。若并发 inject 多条消息，按 cast 顺序依次插入。

### CancelCmd / RunAborted 携带 reason

当前 `CancelCmd` 是空结构体，`RuntimeEvent::RunAborted` 是 unit variant——watcher `Abort(reason)` 的原因会被静默丢弃，事件流里无法区分"watcher 主动终止"和"用户手动 abort"。

v0.8 让取消原因贯穿到事件流：

```rust
// run/actor.rs
pub(crate) struct CancelCmd {
    pub reason: Option<String>,
}

// events.rs
RuntimeEvent::RunAborted { reason: Option<String> },
```

- `RunHandle::abort()`（用户手动）→ `CancelCmd { reason: None }`
- watcher `Abort(r)` → `CancelCmd { reason: Some(r) }`
- actor 的 Cancel handler（`actor.rs:321`，当前为 `AgentMsg::Cancel(_)`）改为提取 reason 并透传：

```rust
AgentMsg::Cancel(cmd) => {
    if !state.cancelled {
        state.cancelled = true;
        emit(&state.event_subs, RuntimeEvent::RunAborted { reason: cmd.reason }).await;
    }
    myself.stop(None);
}
```

**需同步更新的 RunAborted 站点（unit variant → 带字段）：**
- `events.rs:155`：`RunAborted,` → `RunAborted { reason: Option<String> },`
- `actor.rs:324`：emit 处改为 `RunAborted { reason: cmd.reason }`（如上）
- `tests/e2e_validation.rs:312`：match arm `RuntimeEvent::RunAborted =>` → `RuntimeEvent::RunAborted { .. } =>`
- binding crates（py/node）：检查事件序列化/匹配是否覆盖 RunAborted，补 reason 字段

### Watcher Trait

```rust
// run/watcher.rs（新建）

use std::sync::Arc;
use async_trait::async_trait;
use crate::events::RuntimeEvent;

pub enum WatcherAction {
    Continue,
    Inject(String),   // 向 run 注入消息（软干预）
    Abort(String),    // 终止 run（reason 透传到 RunAborted 事件）
}

#[async_trait]
pub trait Watcher: Send + Sync {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction;
}
```

### RunHandle::attach_watcher

```rust
// run/handle.rs

impl RunHandle {
    pub async fn attach_watcher(&self, watcher: Arc<dyn Watcher>, capacity: usize) {
        let mut rx = self.subscribe_events(capacity).await;
        let actor_ref = Arc::clone(&self.actor_ref);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match watcher.on_event(&event).await {
                    WatcherAction::Continue => {}
                    WatcherAction::Inject(msg) => {
                        if let Ok(guard) = actor_ref.lock() {
                            if let Some(ref aref) = *guard {
                                let _ = aref.cast(AgentMsg::Inject(InjectCmd { message: msg }));
                            }
                        }
                    }
                    WatcherAction::Abort(reason) => {
                        if let Ok(guard) = actor_ref.lock() {
                            if let Some(ref aref) = *guard {
                                let _ = aref.cast(AgentMsg::Cancel(CancelCmd { reason: Some(reason) }));
                            }
                        }
                        break; // 发出 abort 后退出 watcher task
                    }
                }
            }
        });
    }
}
```

**设计说明**：
- Watcher task 的生命周期：`rx.recv()` 在 channel 关闭（actor 停止，event_subs 中的 tx drop）后返回 `None`，task 自然退出
- `Inject` 和 `Cancel` 都是单向 `cast`（与 `RunHandle::abort()` 现有写法一致），watcher task 不阻塞等待 actor 回复——注入是 best-effort 软干预
- `Abort(reason)` 的 reason 通过 `CancelCmd.reason` 透传到 `RunAborted` 事件，主订阅者可观测到终止原因

### 模块 re-export

```rust
// run/mod.rs
pub mod watcher;
pub use watcher::{Watcher, WatcherAction};

// src/lib.rs
pub use run::{Watcher, WatcherAction};
```

## 验收标准

- [ ] `RunHandle::subscribe_events(capacity)` 存在，返回 `EventReceiver`
- [ ] `subscribe_events` / `attach_watcher` 在 `actor_ref` 未就绪时 await 至就绪再 cast（紧随 `start()` 调用不丢订阅）
- [ ] 多个次级订阅者可同时接收（订阅后发出的）事件
- [ ] 次级订阅者 channel 满时，`EventsDropped` 事件发到主订阅者（不阻塞 run loop）
- [ ] `InjectCmd.message: String` 字段存在；`AgentMsg::Inject(InjectCmd)` 为单向 cast（无 RpcReplyPort）
- [ ] `Inject` 处理不再返回"not yet implemented"；注入的消息在下一轮 model call 前出现在 messages 中
- [ ] `CancelCmd.reason: Option<String>` 字段存在；`RuntimeEvent::RunAborted { reason }` 携带 reason
- [ ] `RunHandle::abort()` 发送 `CancelCmd { reason: None }`（行为不变，事件 reason 为 None）
- [ ] `Watcher` trait 定义完整，含 `#[async_trait]`，`on_event` 返回 `WatcherAction`
- [ ] `WatcherAction::Inject(msg)` 通过内部 InjectCmd 向 run 注入消息
- [ ] `WatcherAction::Abort(reason)` 向 actor 发送 `Cancel { reason: Some(r) }`，run 以 `RunAborted { reason: Some(r) }` 终止
- [ ] `RunHandle::attach_watcher(watcher, capacity)` 存在，watcher task 可接收 run 事件
- [ ] Watcher task 在 run 结束后自然退出（channel 关闭，不 leak）
- [ ] `AgentMsg::Steer` 仍保留"not yet implemented" stub（v0.9）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `Inject` / `Cancel` 均为单向 `cast`，参考 `RunHandle::abort()` 现有写法；v0.8 不需要背压确认，去掉 v0.7 stub 中 `Inject` 的 `RpcReplyPort`
- Watcher task 持有 `Arc<Mutex<Option<ActorRef<AgentMsg>>>>`，在 run 结束后 Option 变为 None；watcher task 发送时应检查 None 情况
- `EventsDropped` event 中的 `subscriber_id` 是 event_subs 列表中的索引（已由 v0.7 emit 实现），不是 watcher 的标识符
- `RunAborted` 从 unit variant 改为带字段，需同步更新所有构造点（actor.rs 现有的 `emit(..., RuntimeEvent::RunAborted)`）和 binding crates 的事件序列化/匹配
