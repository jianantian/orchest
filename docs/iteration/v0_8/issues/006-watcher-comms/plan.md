# 006 · Multi-subscriber Events + Watcher + InjectCmd — 实施计划

## 前置条件

- `cargo test --workspace` 全绿（基线确认）
- 本 issue 不依赖 001-005

---

## 步骤

### 步骤 1：`run/actor.rs` — InjectCmd payload + Subscribe 变体

**InjectCmd**（替换现有空结构体）：
```rust
pub(crate) struct InjectCmd {
    pub message: String,
}
```

**AgentMsg 新增 Subscribe**：
```rust
pub(crate) enum AgentMsg {
    RunStep,
    Subscribe(mpsc::Sender<RuntimeEvent>),  // 新增
    Steer(SteerCmd, RpcReplyPort<SteerResult>),
    Inject(InjectCmd, RpcReplyPort<()>),
    Cancel(CancelCmd),
}
```

**handle() 中处理 Subscribe**：
```rust
AgentMsg::Subscribe(tx) => {
    if let Some(state) = self.state.as_mut() {
        state.event_subs.push(tx);
    }
    Ok(())
}
```

（actor.rs 的 handle 方法，参考现有 Cancel 的处理结构）

**handle() 中处理 Inject**（替换 "not yet implemented" stub）：
```rust
AgentMsg::Inject(cmd, reply) => {
    if let Some(state) = self.state.as_mut() {
        use crate::model::{Message, Role, ContentBlock};
        state.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: cmd.message }],
        });
    }
    let _ = reply.send(());
    Ok(())
}
```

### 步骤 2：`run/handle.rs` — subscribe_events

```rust
pub async fn subscribe_events(&self, capacity: usize) -> crate::run::EventReceiver {
    let (tx, rx) = tokio::sync::mpsc::channel(capacity);
    if let Ok(guard) = self.actor_ref.lock() {
        if let Some(ref aref) = *guard {
            let _ = aref.cast(AgentMsg::Subscribe(tx));
        }
    }
    rx
}
```

### 步骤 3：新建 `run/watcher.rs`

```rust
use std::sync::Arc;
use async_trait::async_trait;
use crate::events::RuntimeEvent;

pub enum WatcherAction {
    Continue,
    Inject(String),
    Abort(String),
}

#[async_trait]
pub trait Watcher: Send + Sync {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction;
}
```

### 步骤 4：`run/handle.rs` — attach_watcher

考虑到 `Inject` 有 `RpcReplyPort`，watcher 中使用 `ractor::call_t!` 或将 Inject 改为 cast。

**推荐改法**：Inject 保持 `RpcReplyPort<()>` 用于 v0.9 背压，watcher 中用 `call_t!` 加 100ms 超时。`call_t!` 宏来自 ractor，参考 v0.7 actor test 中的用法。

完整 `attach_watcher` 实现（参见 spec）。

注意 `actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>` — Ractor 的 `ActorRef` 需要在 tokio 上下文中使用；watcher task 是 `tokio::spawn`，可以调用 `call_t!`。

若 `call_t!` 宏在 watcher task 中调用有复杂性（需要 ractor runtime），可改为：

```rust
// 简化方案：Inject 改为无 reply 的 cast 消息（不等待确认）
// 在 AgentMsg 中改 Inject 为 cast 语义，或新增 InjectCast 变体
```

v0.8 使用简化方案（InjectCast，无 reply），v0.9 再升级为有背压的 call。

**简化方案的 AgentMsg 调整**：
```rust
// 将 Inject 的 RpcReplyPort 去掉，改为 fire-and-forget
Inject(InjectCmd),  // 去掉 RpcReplyPort<()>
```

handle() 相应修改：
```rust
AgentMsg::Inject(cmd) => {
    if let Some(state) = self.state.as_mut() {
        state.messages.push( /* ... */ );
    }
    Ok(())
}
```

watcher 中 cast：
```rust
WatcherAction::Inject(msg) => {
    if let Some(ref aref) = *guard {
        let _ = aref.cast(AgentMsg::Inject(InjectCmd { message: msg }));
    }
}
```

> **注**：若 spec 要求保留 RpcReplyPort（为 v0.9 背压），则使用 call_t! 方案。在 plan 中保留两种方案，实现时选简化方案，commit 中注明。

### 步骤 5：`run/mod.rs` 和 `src/lib.rs` — 声明和 re-export

```rust
// run/mod.rs
pub mod watcher;
pub use watcher::{Watcher, WatcherAction};

// src/lib.rs
pub use run::{Watcher, WatcherAction};
```

### 步骤 6：单元测试

在 `crates/agent-runtime-core/tests/v08_integration.rs` 或 `run/tests.rs`：

1. `subscribe_events_receives_all_events`：attach 一个次级 subscriber，验证收到 RunStarted 等事件
2. `multiple_subscribers_each_receive_events`：attach 两个次级 subscriber，验证各自独立接收
3. `secondary_subscriber_full_channel_emits_events_dropped`：次级 capacity=1，产生多事件，验证主 subscriber 收到 EventsDropped
4. `inject_message_appears_in_next_model_call`：FakeModelAdapter 记录收到的 messages，attach_watcher 在 RunStarted 时注入消息，验证 FakeModelAdapter 在下一轮收到注入的 user message
5. `watcher_abort_terminates_run`：watcher 在某事件后返回 Abort，验证 run 以 RunAborted 结束
6. `watcher_task_exits_after_run_completes`：run 完成，验证 watcher task channel 关闭（通过 rx.recv() 返回 None 验证）

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
