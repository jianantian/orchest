# 005 · AgentRun Actor Refactor（条件执行）

## 前置条件

**本 issue 仅在 001（Ractor PoC）gate 结论为"通过"或"有条件通过"时执行。**

如果 001 结论为"未通过"，本 issue 跳过，后续 v0.8/v0.9 使用 channel 原语方案。

## 背景

001 PoC 验证了 Ractor 可以满足 Orchest runtime 的通信需求。本 issue 将 PoC 结论落地——将 AgentRun 从 "tokio::spawn + channel" 重构为 Ractor actor，同时保持 `RunHandle` 公共 API 不变。

重构目标是为 v0.8 的双向通信、多方事件订阅、Watcher 注册打下 actor 基础。

## 目标

将 AgentRun 重构为 Ractor actor，在 Ractor 之上封装 typed API，公共 API（`RunHandle`）不变。

## 范围

### AgentRun → WorkerActor

根据 001 PoC 确定的集成模式（A/B/C），将 `run/loop_.rs` 的 run loop 封装到 Ractor actor 中：

```rust
struct WorkerActor;

enum AgentMsg {
    RunStep,                                    // self-message 驱动 loop（模式 B）
    Steer(SteerCmd, RpcReplyPort<SteerResult>),  // v0.8 使用
    Inject(InjectCmd, RpcReplyPort<()>),          // v0.8 使用
    Cancel(CancelCmd),                           // 替代 CancellationToken
}

struct AgentRunState {
    // 现有 RunState 字段迁移
    config: AgentConfig,
    messages: Vec<Message>,
    budget_guard: BudgetGuard,
    step: u32,
    // ...
}

#[async_trait]
impl Actor for WorkerActor {
    type Msg = AgentMsg;
    type State = AgentRunState;
    type Arguments = AgentRunArgs;

    async fn pre_start(&self, myself: ActorRef<AgentMsg>, args: AgentRunArgs)
        -> Result<AgentRunState, ActorProcessingErr> {
        // 初始化 run state，emit RunStarted，self-send RunStep
    }

    async fn handle(&self, myself: ActorRef<AgentMsg>, msg: AgentMsg, state: &mut AgentRunState)
        -> Result<(), ActorProcessingErr> {
        match msg {
            AgentMsg::RunStep => {
                // 执行一步 run loop iteration
                // 完成后 self-send RunStep（除非 run 结束）
            }
            AgentMsg::Steer(cmd, reply) => { /* v0.8 实现 */ }
            AgentMsg::Inject(cmd, reply) => { /* v0.8 实现 */ }
            AgentMsg::Cancel(_) => {
                // 设置 cancel 标志，RunStep 下次检查时退出
            }
        }
    }

    async fn post_stop(&self, _myself: ActorRef<AgentMsg>, state: &mut AgentRunState)
        -> Result<(), ActorProcessingErr> {
        // emit RunCompleted / RunFailed
    }
}
```

`Steer` 和 `Inject` variant 在本 issue 中声明但实现为空（`reply.send(())`），v0.8 填充逻辑。

### RunHandle 内部重构

`RunHandle` 的公共 API 保持不变，内部用 `ActorRef<AgentMsg>` 替代 `JoinHandle + CancellationToken`：

```rust
pub struct RunHandle {
    pub run_id: RunId,
    pub(crate) actor_ref: ActorRef<AgentMsg>,
    pub(crate) approval_bus: ApprovalBus,
    // task: JoinHandle 移除
    // cancel_token: CancellationToken 移除
}

impl RunHandle {
    pub async fn wait(self) {
        // 等待 actor stop
    }

    pub fn abort(&self) {
        cast!(self.actor_ref, AgentMsg::Cancel(CancelCmd));
    }

    pub async fn respond_approval(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        self.approval_bus.respond(run_id, approved).await
    }
}
```

### Typed API 封装层

面向框架用户（不需要知道 Ractor）：

```rust
pub struct AgentRef {
    inner: ActorRef<AgentMsg>,
}

impl AgentRef {
    pub async fn steer(&self, cmd: SteerCmd) -> Result<SteerResult, AgentError> {
        call!(self.inner, AgentMsg::Steer(cmd, _))
            .map_err(|e| AgentError::Communication(e.to_string()))
    }

    pub fn cancel(&self, cmd: CancelCmd) {
        let _ = cast!(self.inner, AgentMsg::Cancel(cmd));
    }
}
```

`AgentRef` 在本 issue 中定义为 `pub(crate)`，v0.8 暴露为 `pub` 并添加集成测试。这是前向声明——类型在 v0.7 就位，v0.8 有调用者后才验证完整 API。

### 事件分发

替换 `mpsc::channel` → actor 事件分发：

- WorkerActor 内部维护 `Vec<mpsc::Sender<RuntimeEvent>>` subscriber list
- `RunHandle` 创建时获得一个 `mpsc::Receiver`（保持现有事件消费 API）
- v0.8 的 `subscribe_events()` 可以向 subscriber list 添加新 consumer

**Backpressure 策略**：subscriber channel 使用 bounded mpsc（容量 256）。当某个 subscriber 的 channel 满时，使用 `try_send`——满则丢弃该事件并发出 `RuntimeEvent::EventsDropped { subscriber_id, count }` 警告。慢 consumer 不能拖死 actor。这是 event stream 的标准做法（lossy > blocking）

### Ractor 依赖引入

在 `agent-runtime-core` 的 `Cargo.toml` 中新增：

```toml
[dependencies]
ractor = { version = "0.15", features = ["message_span_propagation"] }
```

启用 `message_span_propagation` feature 用于 agent trace 传播。

## 需要修改的文件

| 文件 | 变更 |
|------|------|
| `Cargo.toml` | 新增 ractor 依赖 |
| `run/loop_.rs` | run loop 逻辑迁移到 WorkerActor::handle() |
| `run/handle.rs` | RunHandle 内部从 JoinHandle → ActorRef |
| `run/mod.rs` | AgentRun::start() 改为 Actor::spawn() |
| 新增 `run/actor.rs` | WorkerActor / AgentMsg / AgentRunState |
| 新增 `run/agent_ref.rs` | AgentRef typed API |

## 不在范围内

- Steer / Inject 消息的实际处理逻辑 → v0.8
- WatcherAgent 实现 → v0.8/v0.9
- SupervisionEvent 处理 → v0.9（崩溃恢复）
- ractor_cluster / 分布式

## 依赖

- 001（Ractor PoC）：gate 通过
- 002（Hook Framework）：hook 调用点需要在 actor handle() 中正确触发

## 验收标准

- [ ] `ractor` 依赖在 `agent-runtime-core/Cargo.toml` 中
- [ ] `WorkerActor` 实现 Ractor `Actor` trait
- [ ] `AgentMsg` enum 包含 RunStep / Steer / Inject / Cancel
- [ ] `RunHandle` 公共 API 不变（`wait()` / `abort()` / `respond_approval()`）
- [ ] `RunHandle` 内部使用 `ActorRef<AgentMsg>`
- [ ] `AgentRef` typed API 定义为 `pub(crate)`（steer / cancel 方法签名就位）
- [ ] 事件 subscriber backpressure：慢 consumer 不阻塞 actor，满时丢弃并发出 EventsDropped
- [ ] 现有所有 `run/tests.rs` 测试在 actor 架构下通过（行为不变）
- [ ] hook 调用点在 actor handle() 中正确触发
- [ ] Cancel 通过 `AgentMsg::Cancel` 生效，不再依赖 `CancellationToken`
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
