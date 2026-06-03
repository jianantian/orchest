# 005 · AgentRun Actor Refactor — 实施计划

## 前置条件

- 001（Ractor PoC）gate 结论为"通过"或"有条件通过"，且集成模式已确认（预计模式 B：self-message 每步一条）
- 002（Hook Framework）已合入，hook 调用点已存在于 run/loop_.rs
- `cargo test --workspace` 全绿

---

## 步骤

### 步骤 1：引入 ractor 依赖

文件：`crates/agent-runtime-core/Cargo.toml`，`[dependencies]` 段追加：

```toml
ractor = { version = "0.15", features = ["message_span_propagation"] }
```

验证：`cargo build -p agent-runtime-core` 无报错。

---

### 步骤 2：新增 `run/actor.rs` — AgentMsg、AgentRunState、WorkerActor

新建文件 `crates/agent-runtime-core/src/run/actor.rs`。

#### 2.1 消息类型

```rust
pub(crate) struct CancelCmd;
pub(crate) struct SteerCmd;    // v0.8 填充
pub(crate) struct SteerResult; // v0.8 填充
pub(crate) struct InjectCmd;   // v0.8 填充

pub(crate) enum AgentMsg {
    RunStep,
    Steer(SteerCmd, ractor::RpcReplyPort<SteerResult>),
    Inject(InjectCmd, ractor::RpcReplyPort<()>),
    Cancel(CancelCmd),
}

impl ractor::Message for AgentMsg {}
```

#### 2.2 启动参数

```rust
pub(crate) struct AgentRunArgs {
    pub run_id: RunId,
    pub config: AgentConfig,
    pub input: String,
    pub model: Arc<dyn ModelAdapter>,
    pub registry: ToolRegistry,
    pub approval_bus: ApprovalBus,
    pub event_subs: Vec<mpsc::Sender<RuntimeEvent>>,
}
```

#### 2.3 运行状态

将 run_loop_inner（loop_.rs L59–L163）中所有局部变量提升为结构体字段：

```rust
pub(crate) struct AgentRunState {
    pub run_id: RunId,
    pub config: AgentConfig,
    pub model: Arc<dyn ModelAdapter>,
    pub registry: ToolRegistry,
    pub unfiltered_registry: ToolRegistry,
    pub messages: Vec<Message>,
    pub tool_defs: Vec<ToolDef>,
    pub step: u32,
    pub budget: BudgetGuard,
    pub last_compaction_step: Option<u32>,
    pub approval_bus: ApprovalBus,
    pub event_subs: Vec<mpsc::Sender<RuntimeEvent>>,
    pub webhook_runtime: Option<WebhookRuntime>,
    pub cancelled: bool,
    pub run_result: RunResult,
}

pub(crate) enum RunResult { Pending, Completed(Value), Failed(String), Aborted }
```

#### 2.4 WorkerActor

```rust
pub(crate) struct WorkerActor;

#[async_trait]
impl ractor::Actor for WorkerActor {
    type Msg = AgentMsg;
    type State = AgentRunState;
    type Arguments = AgentRunArgs;

    async fn pre_start(&self, myself: ActorRef<AgentMsg>, args: AgentRunArgs)
        -> Result<AgentRunState, ActorProcessingErr>
    {
        // 对应 loop_.rs L69–L163 的初始化段：
        // - 初始化 webhook_runtime、连接 MCP、注册 skill/code tools
        // - 构建 messages、tool_defs、budget
        // - emit RunStarted
        // - myself.cast(AgentMsg::RunStep)?
        // 返回 AgentRunState
    }

    async fn handle(&self, myself: ActorRef<AgentMsg>, msg: AgentMsg, state: &mut AgentRunState)
        -> Result<(), ActorProcessingErr>
    {
        match msg {
            AgentMsg::RunStep => {
                // 对应 loop_.rs L165–L584 的单次循环体：
                // - cancelled 检查（替代 cancel_token.is_cancelled()）
                // - max_steps / budget check
                // - model.complete()（含 retry，006 完成后包装）
                // - tool dispatch
                // - messages.push + step += 1
                // - 未结束：myself.cast(AgentMsg::RunStep)?
                // - 结束：state.run_result = Completed/Failed，不 self-send → actor 停止
            }
            AgentMsg::Cancel(_) => {
                state.cancelled = true;
            }
            AgentMsg::Steer(_, reply) => {
                let _ = reply.send(SteerResult); // v0.8 填充
            }
            AgentMsg::Inject(_, reply) => {
                let _ = reply.send(()); // v0.8 填充
            }
        }
        Ok(())
    }

    async fn post_stop(&self, _myself: ActorRef<AgentMsg>, state: &mut AgentRunState)
        -> Result<(), ActorProcessingErr>
    {
        // 根据 run_result 发出 RunCompleted / RunFailed / RunAborted
        // approval_bus.cancel(run_id).await
        Ok(())
    }
}
```

#### 2.5 事件分发辅助（模块私有）

```rust
fn emit_to_subs(subs: &[mpsc::Sender<RuntimeEvent>], event: RuntimeEvent) {
    let mut dropped = 0u32;
    for sub in subs {
        if sub.try_send(event.clone()).is_err() {
            dropped += 1;
        }
    }
    if dropped > 0 {
        for sub in subs {
            let _ = sub.try_send(RuntimeEvent::EventsDropped { count: dropped });
        }
    }
}
```

subscriber channel 容量：256（bounded mpsc）。

---

### 步骤 3：新增 `run/agent_ref.rs` — AgentRef typed API

新建 `crates/agent-runtime-core/src/run/agent_ref.rs`：

```rust
pub(crate) struct AgentRef {
    inner: ractor::ActorRef<AgentMsg>,
}

impl AgentRef {
    pub(crate) fn cancel(&self) {
        let _ = self.inner.cast(AgentMsg::Cancel(CancelCmd));
    }

    pub(crate) async fn steer(&self, cmd: SteerCmd) -> Result<SteerResult, String> {
        ractor::call!(self.inner, AgentMsg::Steer(cmd, _))
            .map_err(|e| e.to_string())
    }
}
```

`pub(crate)` 可见性——v0.8 改为 `pub` 并补充集成测试。

---

### 步骤 4：修改 `run/handle.rs` — 用 ActorRef 替代 JoinHandle+CancellationToken

文件：`crates/agent-runtime-core/src/run/handle.rs`

当前 `RunHandle` struct（L43–L48）改为：

```rust
pub struct RunHandle {
    pub run_id: RunId,
    pub(crate) actor_ref: ractor::ActorRef<AgentMsg>,
    pub(crate) actor_handle: ractor::ActorHandle,
    pub(crate) approval_bus: ApprovalBus,
}
```

方法实现（L50–L63）：
- `wait(self)`：`let _ = self.actor_handle.await;`
- `abort(&self)`：`let _ = self.actor_ref.cast(AgentMsg::Cancel(CancelCmd));`
- `respond_approval` 保持不变

公共 API 签名无变化。

---

### 步骤 5：修改 `events.rs` — 新增 EventsDropped

文件：`crates/agent-runtime-core/src/events.rs`，在 `RunAborted` 之前追加：

```rust
EventsDropped {
    count: u32,
},
```

---

### 步骤 6：修改 `run/mod.rs` — 改用 Actor::spawn

文件：`crates/agent-runtime-core/src/run/mod.rs`：

1. 新增 `pub(crate) mod actor;` 和 `pub(crate) mod agent_ref;`

2. `start_with_bus_and_token`（L57–L91）逻辑替换：
   ```rust
   let (event_tx, event_rx) = mpsc::channel(256);
   let args = actor::AgentRunArgs { run_id, config, input, model, registry, approval_bus: approval_bus.clone(), event_subs: vec![event_tx] };
   let (actor_ref, actor_handle) =
       ractor::Actor::spawn(None, actor::WorkerActor, args)
           .await
           .expect("actor spawn failed");
   let handle = RunHandle { run_id, actor_ref, actor_handle, approval_bus };
   (handle, event_rx)
   ```

3. 移除 `tokio_util::sync::CancellationToken` 的 use 和 `cancel_token` 参数。

---

### 步骤 7：迁移 loop_.rs 逻辑到 actor.rs

将 `run_loop_inner`（loop_.rs L59–L585）整体移入 WorkerActor 的 `pre_start` + `handle(RunStep)` + `post_stop`：

- `pre_start`：L69–L163（初始化段，不含主循环）
- `handle(RunStep)`：L165–L584（单次 loop 体）
- `post_stop`：RunCompleted/RunFailed/RunAborted 发出 + approval_bus cleanup

loop_.rs 保留文件（防止未来重用），删除 `run_loop` 和 `run_loop_inner` 函数定义。

---

### 步骤 8：迁移 hook 调用点

002 在 loop_.rs 中插入的 hook 调用随逻辑迁移到 actor.rs，保持插入位置不变。

---

### 步骤 9：运行测试修复

```bash
cargo test -p agent-runtime-core
cargo clippy -p agent-runtime-core -- -D warnings
```

重点检查 `run/tests.rs` 中所有已有测试（行为不变，底层换了实现）。

---

## 验证

```bash
# 全量测试
cargo test --workspace

# Clippy
cargo clippy --workspace -- -D warnings

# 确认 ractor 依赖引入
cargo tree -p agent-runtime-core | grep ractor

# 确认 RunHandle 公共 API 签名不变
grep -n "pub fn wait\|pub fn abort\|pub async fn respond_approval" \
  crates/agent-runtime-core/src/run/handle.rs
```

---

## 关键决策

- **模式 B（self-message 每步一条）**：RunStep 驱动每次 loop iteration，Cancel 在下一步前生效，响应及时；避免模式 C 的 select!/mailbox 冲突。
- **ActorHandle for wait()**：ractor::Actor::spawn 返回 (ActorRef, JoinHandle)，JoinHandle 实现 wait()，与原 tokio JoinHandle 语义一致。
- **event_subs 初始容量为单个 Sender（256）**：v0.8 的 subscribe_events() 可追加更多 Sender；慢 consumer 使用 try_send（lossy），满时发出 EventsDropped。
- **Steer/Inject 空实现**：v0.7 声明类型和签名，v0.8 填充逻辑，避免破坏性变更。
