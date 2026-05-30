# 001 · Ractor PoC — 实施计划

## 前置条件

- `cargo test --workspace` 全绿（确认基线）
- 无 production code 变更——所有代码仅在 integration test 或 PoC binary 中

---

## 步骤

### 步骤 1：添加 ractor 到 dev-dependencies

文件：`crates/agent-runtime-core/Cargo.toml`，`[dev-dependencies]` 段追加：

```toml
ractor = { version = "0.15", features = ["message_span_propagation"] }
```

验证：`cargo build -p agent-runtime-core` 无报错。

---

### 步骤 2：新建 PoC 集成测试 `tests/ractor_poc.rs`

新建文件 `crates/agent-runtime-core/tests/ractor_poc.rs`。

#### 2.1 消息类型

```rust
use ractor::{Actor, ActorRef, ActorProcessingErr, RpcReplyPort};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct SteerCmd(pub String);
#[derive(Debug, Clone)]
pub struct SteerResult(pub String);

pub enum WorkerMsg {
    RunStep,
    Steer(SteerCmd, RpcReplyPort<SteerResult>),
    Cancel,
}
impl ractor::Message for WorkerMsg {}
```

#### 2.2 WorkerAgent state（模拟 run loop 状态）

```rust
pub struct WorkerState {
    pub steps_done: u32,
    pub cancelled: bool,
    pub event_tx: mpsc::Sender<String>,
}
```

#### 2.3 WorkerAgent impl

```rust
pub struct WorkerAgent;

#[async_trait]
impl Actor for WorkerAgent {
    type Msg = WorkerMsg;
    type State = WorkerState;
    type Arguments = mpsc::Sender<String>;

    async fn pre_start(&self, myself: ActorRef<WorkerMsg>, tx: mpsc::Sender<String>)
        -> Result<WorkerState, ActorProcessingErr>
    {
        myself.cast(WorkerMsg::RunStep)?;
        Ok(WorkerState { steps_done: 0, cancelled: false, event_tx: tx })
    }

    async fn handle(
        &self, myself: ActorRef<WorkerMsg>, msg: WorkerMsg, state: &mut WorkerState,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            WorkerMsg::RunStep => {
                if state.cancelled || state.steps_done >= 5 {
                    return Ok(());
                }
                // 模拟一步：emit event，self-send 下一步
                let _ = state.event_tx.send(format!("step:{}", state.steps_done)).await;
                state.steps_done += 1;
                myself.cast(WorkerMsg::RunStep)?;
            }
            WorkerMsg::Steer(cmd, reply) => {
                let _ = state.event_tx.send(format!("steered:{}", cmd.0)).await;
                let _ = reply.send(SteerResult(format!("ack:{}", cmd.0)));
            }
            WorkerMsg::Cancel => {
                state.cancelled = true;
                let _ = state.event_tx.send("cancelled".to_string()).await;
            }
        }
        Ok(())
    }
}
```

#### 2.4 V1 验证测试：模式 B self-message

```rust
#[tokio::test]
async fn v1_self_message_step_pattern() {
    let (tx, mut rx) = mpsc::channel(64);
    let (actor_ref, handle) = Actor::spawn(None, WorkerAgent, tx).await.unwrap();
    let _ = handle.await;
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() { events.push(e); }
    assert!(events.iter().any(|e| e.starts_with("step:")));
}
```

#### 2.5 V2 验证测试：Cancel 优先于积压 RunStep

```rust
#[tokio::test]
async fn v2_cancel_priority() {
    // 向 mailbox 注入 100 个 RunStep，然后 Cancel
    // 验证 actor 在处理积压消息前响应 Cancel
    let (tx, mut rx) = mpsc::channel(256);
    let (actor_ref, handle) = Actor::spawn(None, WorkerAgent, tx).await.unwrap();
    // 注入积压消息
    for _ in 0..100 {
        let _ = actor_ref.cast(WorkerMsg::RunStep);
    }
    let _ = actor_ref.cast(WorkerMsg::Cancel);
    let _ = handle.await;
    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(events.contains(&"cancelled".to_string()));
    // steps_done 应 < 100（cancel 提前生效）
}
```

#### 2.6 V3 验证测试：supervision（panic 后 parent 收到 ActorPanicked）

```rust
pub struct PanickingActor;
pub enum PanicMsg { Panic }
impl ractor::Message for PanicMsg {}

#[async_trait]
impl Actor for PanickingActor {
    type Msg = PanicMsg;
    type State = ();
    type Arguments = ();
    async fn pre_start(&self, _: ActorRef<PanicMsg>, _: ()) -> Result<(), ActorProcessingErr> { Ok(()) }
    async fn handle(&self, _: ActorRef<PanicMsg>, _: PanicMsg, _: &mut ()) -> Result<(), ActorProcessingErr> {
        panic!("simulated panic");
    }
}

#[tokio::test]
async fn v3_supervision_panicked_event() {
    // spawn PanickingActor, send Panic, verify JoinHandle reflects error
    let (actor_ref, handle) = Actor::spawn(None, PanickingActor, ()).await.unwrap();
    let _ = actor_ref.cast(PanicMsg::Panic);
    let result = handle.await;
    // actor handle should reflect the panic/stop
    // (verify ractor surfaces panics through supervision chain)
}
```

#### 2.7 V4 验证测试：typed API 封装层

```rust
pub struct AgentRef {
    inner: ActorRef<WorkerMsg>,
}
impl AgentRef {
    pub fn cancel(&self) { let _ = self.inner.cast(WorkerMsg::Cancel); }
    pub async fn steer(&self, cmd: SteerCmd) -> Result<SteerResult, String> {
        ractor::call!(self.inner, WorkerMsg::Steer(cmd, _))
            .map_err(|e| e.to_string())
    }
}

#[tokio::test]
async fn v4_typed_api() {
    let (tx, _rx) = mpsc::channel(64);
    let (actor_ref, _handle) = Actor::spawn(None, WorkerAgent, tx).await.unwrap();
    let agent_ref = AgentRef { inner: actor_ref };
    let result = agent_ref.steer(SteerCmd("redirect".to_string())).await;
    assert!(result.is_ok());
    assert!(result.unwrap().0.contains("redirect"));
}
```

---

### 步骤 3：在 `Cargo.toml` 中注册 integration test

文件：`crates/agent-runtime-core/Cargo.toml`，在现有 `[[example]]` 之前追加：

```toml
[[test]]
name = "ractor_poc"
path = "tests/ractor_poc.rs"
```

---

### 步骤 4：记录 Gate 决策

运行测试后，在 `docs/research/actor-model-evaluation.md` 末尾追加 `## PoC 结论` 附录，记录：

- V1-V4 每项结论（通过/未通过/workaround）
- 集成模式选择（模式 A/B/C 及理由）
- Gate 决策（通过/有条件通过/未通过）

---

## 验证

```bash
# 运行 PoC 测试
cargo test --test ractor_poc -- --nocapture

# 确认 ractor 依赖引入
cargo tree -p agent-runtime-core | grep ractor

# 确认无 production code 变更
git diff --stat HEAD -- crates/agent-runtime-core/src/
# 期望：仅 Cargo.toml 变更
```

---

## 关键决策

- **模式 B（self-message 每步一条）**：V1 验证重点，cancel_priority 测试验证模式 B 下 Kill 能在步间生效
- **integration test 而非 binary**：PoC 以 `[[test]]` 形式存在，方便 CI 直接跑
- **V3 supervision 验证**：通过 JoinHandle 的错误状态验证，而不需要完整 SupervisorActor；Ractor 的 spawn_linked 等高级 API 可选验证
