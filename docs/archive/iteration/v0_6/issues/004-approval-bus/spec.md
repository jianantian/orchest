# 004 · ApprovalBus + AgentDelegate 路径统一

## 背景

当前有两个并行的 sub-agent 执行路径，以及一个有竞争风险的 approval 路由机制：

### 问题 1：两条 sub-agent 路径

`run.rs` 里存在两套 sub-agent 调度机制：

1. **`ToolOutput::AgentDelegate` 路径**（`run.rs:836`）：调用 `execute_agent_delegate()`，这是"正规"路径
2. **`__sub_agent_request` 魔法字段路径**（`run.rs:778`）：从 `ToolOutput::Immediate` 里检测魔法键，调用 `execute_sub_agent_request()`

两个函数逻辑几乎重复，测试中也用到了 `__sub_agent_request` 方式（`run.rs:3836`）。这是历史遗留，应统一到 `AgentDelegate`。

### 问题 2：`active_children` 手动维护

`RunHandle` 用 `Arc<Mutex<HashMap<RunId, ApprovalSlot>>>` 跟踪活跃子 agent：

```rust
pub struct RunHandle {
    pending_approval: ApprovalSlot,
    active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
}
```

问题：
- 父只能路由到直接子，孙 agent 的 approval 无法穿透
- 子 agent 结束后如果没有及时从 map 清除，`respond_approval` 会向一个已死的 sender 发送

## 目标

1. 用 `ApprovalBus`（全局 ID 注册表）替代 `active_children` HashMap
2. 消除 `__sub_agent_request` 魔法字段和 `execute_sub_agent_request` 函数，所有 sub-agent 统一走 `ToolOutput::AgentDelegate`
3. Sub-agent 运行时与父共享同一个 `ApprovalBus` 实例，任意深度的 approval 都可通过 `RunHandle::respond_approval(run_id, bool)` 路由

## ApprovalBus 设计

文件：`run/handle.rs`

```rust
/// Shared approval registry for an entire agent-run tree.
/// All runs (root + sub-agents at any depth) share the same instance.
#[derive(Clone, Default)]
pub struct ApprovalBus {
    pending: Arc<Mutex<HashMap<RunId, oneshot::Sender<bool>>>>,
}

impl ApprovalBus {
    /// Register a pending approval. Returns a receiver the run should await.
    /// Calling this twice for the same run_id replaces the old sender.
    pub async fn request(&self, run_id: RunId) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(run_id, tx);
        rx
    }

    /// Deliver a verdict to a waiting run. Returns Err if no run is waiting.
    pub async fn respond(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        match self.pending.lock().await.remove(&run_id) {
            Some(tx) => tx.send(approved)
                .map_err(|_| format!("run {run_id} is no longer waiting for approval")),
            None => Err(format!("no pending approval for run {run_id}")),
        }
    }

    /// Auto-cleanup when a run exits without consuming its approval slot.
    pub async fn cancel(&self, run_id: RunId) {
        self.pending.lock().await.remove(&run_id);
    }
}

pub struct RunHandle {
    pub run_id: RunId,
    task: tokio::task::JoinHandle<()>,
    approval_bus: ApprovalBus,
}

impl RunHandle {
    /// Route an approval to any run in the tree by RunId (root or sub-agent).
    pub async fn respond_approval(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        self.approval_bus.respond(run_id, approved).await
    }

    pub async fn wait(self) {
        let _ = self.task.await;
    }
}
```

## AgentRun::start 变更

```rust
// run/mod.rs
impl AgentRun {
    pub fn start(
        input: String,
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        let bus = ApprovalBus::default();
        Self::start_with_bus(input, config, model, registry, bus)
    }

    pub(crate) fn start_with_bus(
        input: String,
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        bus: ApprovalBus,
    ) -> (RunHandle, EventReceiver) {
        let run_id = RunId::new();
        let (event_tx, event_rx) = mpsc::channel(256);
        let bus_clone = bus.clone();
        let task = tokio::spawn(async move {
            run_loop(run_id, config, input, model, registry, event_tx, bus_clone).await;
        });
        let handle = RunHandle { run_id, task, approval_bus: bus };
        (handle, event_rx)
    }
}
```

## sub_agent.rs：统一执行路径

文件：`run/sub_agent.rs`

```rust
/// Execute an AgentDelegate as a sub-agent, sharing the parent's ApprovalBus.
/// Returns (model_output, details) matching the existing call site contract.
pub(super) async fn execute_agent_delegate(
    parent_run_id: RunId,
    delegate: Box<AgentDelegate>,
    approval_bus: ApprovalBus,
    parent_event_tx: &mpsc::Sender<RuntimeEvent>,
    parent_budget: &mut BudgetGuard,
) -> (Value, Value) {
    let (handle, mut event_rx) = AgentRun::start_with_bus(
        delegate.input,
        delegate.config,
        delegate.model,
        delegate.registry,
        approval_bus,   // ← 共享，不新建
    );

    let mut child_output = json!(null);

    while let Some(event) = event_rx.recv().await {
        // 向上转发 sub-agent 事件
        let _ = parent_event_tx.send(RuntimeEvent::SubAgentEvent {
            parent_run_id,
            child_run_id: handle.run_id,
            event: Box::new(event.clone()),
        }).await;

        match &event {
            RuntimeEvent::RunCompleted { output } => child_output = output.clone(),
            RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                parent_budget.record_external_usage(&BudgetUsage {
                    tokens_used: tokens.input_tokens + tokens.output_tokens,
                    tool_calls_used: 0,
                    cost_usd: tokens.cost_usd.unwrap_or(0.0),
                });
            }
            _ => {}
        }
    }
    handle.wait().await;

    let model_output = (delegate.output_mapper)(child_output.clone());
    let details = child_output;
    (model_output, details)
}
```

## RuntimeEvent 新增 SubAgentEvent

文件：`events.rs`

```rust
SubAgentEvent {
    parent_run_id: RunId,
    child_run_id: RunId,
    event: Box<RuntimeEvent>,
},
```

## loop_.rs：approval gate 改用 ApprovalBus

```rust
// 原来：向 pending_approval slot 写入 sender
// 现在：向 approval_bus 注册，等待 receiver
let rx = approval_bus.request(run_id).await;
emit(&tx, RuntimeEvent::ApprovalRequested { tool_call: tool_call.clone() }).await;
let approved = rx.await.unwrap_or(false);
// respond() 内部已调用 remove()，slot 已清除；
// 若 run 在等待期间退出，需在退出路径调用 bus.cancel(run_id) 做兜底清理
```

## 清理

- [ ] 删除 `execute_sub_agent_request` 函数（`run.rs:918`，约 190 行）
- [ ] 删除 `run_loop` 中检测 `__sub_agent_request` 的分支（`run.rs:778-789`）
- [ ] 删除 `active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>>` 字段
- [ ] 删除 `ApprovalSlot` type alias（已被 `ApprovalBus` 取代）
- [ ] 删除 `take_and_send` helper 函数

## 验收标准

### API

- [ ] `RunHandle` 不再包含 `active_children` 字段
- [ ] `RunHandle::respond_approval(run_id, bool)` 接口不变（签名相同）
- [ ] `AgentRun::start()` 对外签名不变
- [ ] `ApprovalBus` 类型在 `crate::run` 模块中 pub 可见（供测试和 FFI 层使用）

### 功能

- [ ] `run/` 目录中不存在字符串 `__sub_agent_request`（grep 验证）
- [ ] `execute_sub_agent_request` 函数不存在
- [ ] `RuntimeEvent::SubAgentEvent` 变体存在
- [ ] Sub-agent 的 `ApprovalRequested` 事件可被根 `RunHandle::respond_approval` 响应

### 单元测试（`run/handle.rs` 内嵌）

- [ ] ApprovalBus 基本 request/respond 测试：
  ```rust
  #[tokio::test]
  async fn approval_bus_round_trip() {
      let bus = ApprovalBus::default();
      let run_id = RunId::new();
      let rx = bus.request(run_id).await;
      bus.respond(run_id, true).await.unwrap();
      assert_eq!(rx.await.unwrap(), true);
  }
  ```
- [ ] ApprovalBus respond 到不存在的 run_id 返回 Err：
  ```rust
  #[tokio::test]
  async fn approval_bus_unknown_run_id_returns_err() {
      let bus = ApprovalBus::default();
      let result = bus.respond(RunId::new(), true).await;
      assert!(result.is_err());
  }
  ```
- [ ] ApprovalBus cancel 后 respond 返回 Err：
  ```rust
  #[tokio::test]
  async fn approval_bus_cancel_clears_slot() {
      let bus = ApprovalBus::default();
      let run_id = RunId::new();
      let _rx = bus.request(run_id).await;
      bus.cancel(run_id).await;
      let result = bus.respond(run_id, true).await;
      assert!(result.is_err());
  }
  ```

### 集成测试

- [ ] 现有 `e2e_validation.rs` 和 `v03_runtime.rs` 中的 sub-agent 相关测试通过；允许将 mock tool 从返回 `__sub_agent_request` 字段改为返回 `ToolOutput::AgentDelegate`，以及更新 config 构建方式以适配 002 的 AgentConfig 变化
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
