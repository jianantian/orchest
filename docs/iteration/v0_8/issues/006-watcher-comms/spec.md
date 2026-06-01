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

Actor `handle()` 中处理 `Subscribe`：将 tx 追加到 `state.event_subs`：

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

### InjectCmd — 补充 payload

```rust
// run/actor.rs

pub(crate) struct InjectCmd {
    pub message: String,
}
```

Actor `handle()` 中处理 `Inject`（替换现有"not yet implemented" stub）：

```rust
AgentMsg::Inject(cmd, reply) => {
    // 将消息插入 messages，以 User role 注入，下一个 RunStep 时模型可见
    state.messages.push(crate::model::Message {
        role: crate::model::Role::User,
        content: vec![crate::model::ContentBlock::Text { text: cmd.message }],
    });
    let _ = reply.send(());
    Ok(())
}
```

**Inject 的时序**：actor mailbox 是 FIFO，`Inject` 消息在队列中排队；实际插入时机取决于当时的 mailbox 状态。通常情况下，`Inject` 在下一个 `RunStep` 开始前被处理（RunStep 消息由上一个 RunStep 结束时 cast，Inject 排在其后）。若并发 inject 多条消息，按 cast 顺序依次插入。

### Watcher Trait

```rust
// run/watcher.rs（新建）

use std::sync::Arc;
use async_trait::async_trait;
use crate::events::RuntimeEvent;

pub enum WatcherAction {
    Continue,
    Inject(String),   // 向 run 注入消息（软干预）
    Abort(String),    // 终止 run
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
        let rx = self.subscribe_events(capacity).await;
        let actor_ref = Arc::clone(&self.actor_ref);
        tokio::spawn(async move {
            let mut rx = rx;
            while let Some(event) = rx.recv().await {
                match watcher.on_event(&event).await {
                    WatcherAction::Continue => {}
                    WatcherAction::Inject(msg) => {
                        // 向 actor 发送 Inject
                        if let Ok(guard) = actor_ref.lock() {
                            if let Some(ref aref) = *guard {
                                // call! 需要 reply；用 cast! 的 fire-and-forget 版本
                                // Inject 有 RpcReplyPort，需要 call!
                                // 简化：不等待回复（fire-and-forget 语义）
                                // 用 ractor::call_t! 设超时，或改 Inject 为 cast（单向）
                                let _ = ractor::call_t!(
                                    aref,
                                    AgentMsg::Inject,
                                    InjectCmd { message: msg },
                                    100  // 100ms timeout
                                );
                            }
                        }
                    }
                    WatcherAction::Abort(reason) => {
                        if let Ok(guard) = actor_ref.lock() {
                            if let Some(ref aref) = *guard {
                                let _ = aref.cast(AgentMsg::Cancel(CancelCmd));
                            }
                        }
                        // 发出 abort 后退出 watcher task
                        break;
                    }
                }
            }
        });
    }
}
```

**设计说明**：
- Watcher task 的生命周期：`rx.recv()` 在 channel 关闭（actor 停止，event_subs 中的 tx drop）后返回 `None`，task 自然退出
- `call_t!` 的 100ms timeout：若 actor 过忙无法及时回复，inject 可能丢失；这是可接受的 best-effort 语义（v0.8 不保证 inject 必达）
- 如果 `Inject` 语义不需要回复确认，可将 actor 消息改为单向 `cast`。选择保留 `RpcReplyPort` 以便 v0.9 添加背压控制

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
- [ ] 多个次级订阅者可同时接收事件
- [ ] 次级订阅者 channel 满时，`EventsDropped` 事件发到主订阅者（不阻塞 run loop）
- [ ] `InjectCmd.message: String` 字段存在
- [ ] `Inject` 处理不再返回"not yet implemented"；注入的消息在下一轮 model call 前出现在 messages 中
- [ ] `Watcher` trait 定义完整，含 `#[async_trait]`，`on_event` 返回 `WatcherAction`
- [ ] `WatcherAction::Inject(msg)` 通过内部 InjectCmd 向 run 注入消息
- [ ] `WatcherAction::Abort(reason)` 向 actor 发送 Cancel，正确终止 run
- [ ] `RunHandle::attach_watcher(watcher, capacity)` 存在，watcher task 可接收 run 事件
- [ ] Watcher task 在 run 结束后自然退出（channel 关闭，不 leak）
- [ ] `AgentMsg::Steer` 仍保留"not yet implemented" stub（v0.9）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `actor_ref` 的锁定和 Ractor `call_t!` 宏的使用：参考 `RunHandle::abort()` 的现有写法（`cast!`）；`Inject` 若要 fire-and-forget，最简方式是把 Inject 改为无 reply 的 cast 消息，v0.8 不需要背压确认
- Watcher task 持有 `Arc<Mutex<Option<ActorRef<AgentMsg>>>>`，在 run 结束后 Option 变为 None；watcher task 发送时应检查 None 情况
- `EventsDropped` event 中的 `subscriber_id` 是 event_subs 列表中的索引（已由 v0.7 emit 实现），不是 watcher 的标识符
