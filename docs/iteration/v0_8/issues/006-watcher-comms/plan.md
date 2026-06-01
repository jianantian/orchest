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

**AgentMsg 调整**（新增 Subscribe；Inject 去掉 RpcReplyPort 改为单向；Cancel 携带 reason）：
```rust
pub(crate) enum AgentMsg {
    RunStep,
    Subscribe(mpsc::Sender<RuntimeEvent>),         // 新增
    Steer(SteerCmd, RpcReplyPort<SteerResult>),    // 保留（v0.9）
    Inject(InjectCmd),                             // v0.8：去掉 RpcReplyPort，单向 cast
    Cancel(CancelCmd),                             // CancelCmd 新增 reason 字段
}

pub(crate) struct InjectCmd { pub message: String }
pub(crate) struct CancelCmd { pub reason: Option<String> }
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

**handle() 中处理 Inject**（替换 "not yet implemented" stub，单向无 reply）：
```rust
AgentMsg::Inject(cmd) => {
    if let Some(state) = self.state.as_mut() {
        use crate::model::{Message, Role, ContentBlock};
        state.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: cmd.message }],
        });
    }
    Ok(())
}
```

**handle() 中处理 Cancel**（透传 reason 到 RunAborted）：现有 Cancel handler 在发出 `RuntimeEvent::RunAborted` 时改为 `RunAborted { reason: cmd.reason }`（需先把 reason 从 CancelCmd 取出）。

**events.rs**：`RunAborted` 从 unit variant 改为 `RunAborted { reason: Option<String> }`，更新所有构造点。

**handle.rs `abort()`**：`aref.cast(AgentMsg::Cancel(CancelCmd { reason: None }))`。

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

`Inject` 和 `Cancel` 都用单向 `cast`（与现有 `RunHandle::abort()` 写法一致），watcher task 不阻塞等待回复。完整实现见 spec：

```rust
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
    break;
}
```

`actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>` — watcher task 是 `tokio::spawn`，cast 不需要等待 ractor runtime 回复。run 结束后 Option 变 None，发送时检查。

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
5. `watcher_abort_terminates_run_with_reason`：watcher 在某事件后返回 `Abort("policy violation")`，验证 run 以 `RunAborted { reason: Some("policy violation") }` 结束
6. `manual_abort_has_no_reason`：`RunHandle::abort()` 触发，验证 `RunAborted { reason: None }`
7. `watcher_task_exits_after_run_completes`：run 完成，验证 watcher task channel 关闭（通过 rx.recv() 返回 None 验证）

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
