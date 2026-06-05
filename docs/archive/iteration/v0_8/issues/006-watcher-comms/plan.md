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

> **重要**：Ractor `handle()` 签名为 `async fn handle(&self, myself, msg: AgentMsg, state: &mut AgentRunState)`（`actor.rs:282`）。`state` 是参数，直接用——**不是** `self.state.as_mut()`。

**handle() 中处理 Subscribe**：
```rust
AgentMsg::Subscribe(tx) => {
    state.event_subs.push(tx);
    Ok(())
}
```

**handle() 中处理 Inject**（替换 "not yet implemented" stub，单向无 reply）：
```rust
AgentMsg::Inject(cmd) => {
    use crate::model::{Message, Role, ContentBlock};
    state.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text { text: cmd.message }],
    });
    Ok(())
}
```

**handle() 中处理 Cancel**（现有 `actor.rs:321` 为 `AgentMsg::Cancel(_)`，改为提取 reason）：
```rust
AgentMsg::Cancel(cmd) => {
    if !state.cancelled {
        state.cancelled = true;
        emit(&state.event_subs, RuntimeEvent::RunAborted { reason: cmd.reason }).await;
    }
    myself.stop(None);
}
```

**RunAborted unit→struct 的三个站点（缺一不可，否则编译失败）**：
- `events.rs:155`：`RunAborted,` → `RunAborted { reason: Option<String> },`
- `actor.rs:324`：emit 处如上
- `tests/e2e_validation.rs:312`：`RuntimeEvent::RunAborted =>` → `RuntimeEvent::RunAborted { .. } =>`
- binding crates（py/node）：grep `RunAborted`，若有匹配/序列化补 reason

**handle.rs `abort()`**：`aref.cast(AgentMsg::Cancel(CancelCmd { reason: None }))`。

### 步骤 2：actor_ref 就绪信号 + subscribe_events

**先解决 actor_ref 就绪竞态**（见 spec）：`start_with_bus`（`run/mod.rs`）的 `actor_ref` 在后台任务里才设置，紧随 `start()` 的 subscribe 会撞到 `None` 而静默丢订阅。

在 `RunHandle` 增加就绪信号。最简：`Arc<tokio::sync::Notify>`，`start_with_bus` 在 `*guard = Some(aref)` 后调用 `ready.notify_waiters()`；`RunHandle` 持有 `ready: Arc<Notify>`。`subscribe_events` 先确保就绪再 cast：

```rust
pub async fn subscribe_events(&self, capacity: usize) -> crate::run::EventReceiver {
    let (tx, rx) = tokio::sync::mpsc::channel(capacity);
    // 等待 actor_ref 就绪（避免紧随 start() 时为 None 而丢订阅）
    loop {
        // 先登记 notified()，再检查，避免错过通知
        let notified = self.ready.notified();
        if self.actor_ref.lock().ok().and_then(|g| g.clone()).is_some() {
            break;
        }
        notified.await;
    }
    if let Ok(guard) = self.actor_ref.lock() {
        if let Some(ref aref) = *guard {
            let _ = aref.cast(AgentMsg::Subscribe(tx));
        }
    }
    rx
}
```

> `ActorRef` 是 `Clone`，`g.clone()` 取出判断就绪即可。`notified()` 必须在检查前登记以避免 notify 在检查与 await 之间丢失。`start_with_bus` 中即使 spawn 失败也应 `notify_waiters()`（或设超时）防止 subscribe 永久挂起——简单起见可在后台任务的 spawn 结果分支都通知。

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

1. `subscribe_events_receives_subsequent_events`：attach 一个次级 subscriber，验证收到订阅后发出的事件（如 `RunCompleted`）。**不要**断言 `RunStarted`——次级订阅者必然错过它（见 spec actor_ref 就绪时序）
2. `multiple_subscribers_each_receive_events`：attach 两个次级 subscriber，验证各自独立接收后续事件
3. `secondary_subscriber_full_channel_emits_events_dropped`：次级 capacity=1，产生多事件，验证主 subscriber 收到 EventsDropped
4. `inject_message_appears_in_next_model_call`：FakeModelAdapter 多轮脚本，watcher 在**运行中事件**（如首个 `ModelCallCompleted` 或 `ToolCallCompleted`，非 RunStarted）触发 Inject，验证 FakeModelAdapter 在后续轮收到注入的 user message
5. `watcher_abort_terminates_run_with_reason`：watcher 在某事件后返回 `Abort("policy violation")`，验证 run 以 `RunAborted { reason: Some("policy violation") }` 结束
6. `manual_abort_has_no_reason`：`RunHandle::abort()` 触发，验证 `RunAborted { reason: None }`
7. `watcher_task_exits_after_run_completes`：run 完成，验证 watcher task channel 关闭（通过 rx.recv() 返回 None 验证）

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
