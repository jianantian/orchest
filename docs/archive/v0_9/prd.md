# v0.9 Spec：Supervised Delegation 端到端

## 背景

v0.7-v0.8 完成了扩展性地基（Hook 框架）、sub-agent 语义统一（Agent-as-Tool + Handoff）、持久化（Session）、安全层（Guardrail + 权限模型）、以及 Supervised Delegation 的基础通信层（双向通信、多方事件订阅、Watcher 注册）。

v0.8 已落地的关键基础设施：
- `AgentMsg::Inject` handler：将消息 push 到 `state.messages`，模型下一轮可见
- `AgentMsg::Steer` 消息枚举 + RPC 通路（handler 为 stub，返回 "not yet implemented"）
- `AgentRef::steer()` / `AgentRef::inject()`：`pub(crate)` 内部 API
- `RunHandle::attach_watcher()`：完整实现，支持 `WatcherAction::Inject` 和 `Abort` 分发
- `WatcherAction` 枚举：`Continue / Inject / Abort`
- Inject 消息已在 `state.messages` 中，`SessionSnapshot` 保存 messages → **注入消息的持久化已自动覆盖**

v0.9 聚焦一件事：**Supervised Delegation 端到端可用**。这是 [Actor Model 评估](../../research/actor-model-evaluation.md) 识别的核心验证 case，也是 Orchest 与竞品差异化的关键能力。

在实现 SD 核心功能之前，先完成一组 API 改造（源自 [外部研究综合分析](../../analysis/external-research-synthesis.md)）。这些改动不是独立的 API 美化，而是 SD 实现的前置依赖——LlmWatcher 需要结构化错误来判断是否干预，`inherit_context` 是 SD 示例的基础能力。

Provider 扩展、文档、发布准备拆到卫星迭代（[v0.9.1](../../iteration/v0_9_1/prd.md)、[v0.9.2](../../iteration/v0_9_2/prd.md)），不阻塞主线。

## 目标

1. API 改造：`Approval` 枚举、结构化 `ToolError`、`as_tool()` Builder、事件增强
2. `RunHandle` 暴露公共 Steering API（`inject_message` + `steer`），补齐 v0.8 stub
3. `LlmWatcher` 可用——watcher LLM 监控 worker 事件流并自主干预
4. Ractor supervision 崩溃恢复——worker panic 后根据策略 restart，从 snapshot 恢复
5. 多 watcher FIFO 协调——Abort 立即生效，Inject 按时间序到达
6. Supervised Delegation 端到端示例可运行

## 范围

---

### Phase 1：前置 API 改造

SD 实现依赖以下 4 项 API 改进。先改 API 形状，再在其上构建 SD 功能。

#### 1A. `Approval` 枚举替代 `requires_approval: bool`

**当前**：`ToolMetadata.requires_approval: bool`，per-tool 只能表达"要/不要"。用户想区分"只读不审批 / 写入看策略 / 外部通信必须审批"时没有自然表达。

**改为**：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Approval {
    Never,          // 只读、搜索、计算——不需要审批
    #[default]
    WhenRisky,      // 写入内部数据——由 run-level 策略决定
    Always,         // 外部通信、金融、破坏性——总是审批
}

pub struct ToolMetadata {
    pub side_effect: bool,
    pub approval: Approval,        // 替代 requires_approval: bool
    pub cost_hint: Option<CostHint>,
    pub timeout: Option<Duration>,
    pub max_output_tokens: Option<u64>,
    pub source: ToolSource,
}
```

**与 `ApprovalMode` 的关系**：
- `PerTool` → 读取 `metadata.approval`（三态枚举取代 bool）
- `None` / `All` → 保留为 run-level override
- `SideEffectOnly` → 被 `Approval::WhenRisky` + `side_effect: bool` 组合语义取代。v0.9 中保留变体但标记 `#[deprecated]`，内部映射为 `PerTool`（此时 `side_effect: true` 的 tool 默认 `Approval::WhenRisky`，效果等价）。Python SDK 的 `"side_effect_only"` 和 Node SDK 的 `"sideEffectOnly"` 继续解析但发出 deprecation warning。正式移除留到下一个 breaking change 窗口。

**`should_approve` 新逻辑**：

```rust
impl RuntimeConfig {
    pub fn should_approve(&self, meta: &ToolMetadata) -> bool {
        if let Some(f) = &self.custom_approval_fn {
            return f(meta);
        }
        match self.approval_mode {
            ApprovalMode::PerTool => match meta.approval {
                Approval::Never => false,
                Approval::WhenRisky => meta.side_effect, // 有副作用且未明确分类 → 审批
                Approval::Always => true,
            },
            ApprovalMode::None => false,
            ApprovalMode::All => true,
            #[allow(deprecated)]
            ApprovalMode::SideEffectOnly => meta.side_effect, // 兼容：等价于旧行为
        }
    }
}
```

**迁移**：现有 `requires_approval: true` → `Approval::Always`，`false` → `Approval::Never`。所有 built-in tool 和 MCP tool 的构造处更新。

**影响范围**：`ToolMetadata` 结构体、`RuntimeConfig::should_approve()`、Hook 的 `ToolHookContext`、SDK 绑定（Python / Node）。

#### 1B. `ToolError` 结构化

**当前**：`ToolError { message: String, code: Option<String> }`——失败分类只能正则匹配 message。

**改为**：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    pub message: String,
    pub kind: ErrorKind,
    pub retry: RetryHint,
    pub code: Option<String>,       // 保留：provider-specific 错误码（MCP bridge、SDK 使用）
    pub next_step: Option<String>,  // 告诉模型接下来可以尝试什么
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ErrorKind {
    InvalidInput,   // 输入不合法 → 模型修正输入
    NotSupported,   // 工具不支持此操作 → 可能是 spec gap
    Transient,      // 超时/限流 → 重试
    #[default]
    Fatal,          // 内部错误/权限 → 不重试，上报
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RetryHint {
    Safe,           // 幂等操作，可以安全重试
    Caution,        // 可能重复但有幂等键保护
    #[default]
    Unsafe,         // 不可重试（支付、发送、删除）
}
```

**`code` 字段保留说明**：`ErrorKind` 是语义分类（4 种），`code` 是 provider-specific 的错误标识（MCP 协议定义、SDK 自定义等），两者正交。移除 `code` 会破坏 MCP bridge 的错误透传。

**SD 收益**：
- `LlmWatcher` 评估事件时可直接读取 `error.kind` 判断是否需要干预（`Transient` → 不干预等重试，`Fatal` → 可能需要 steer）
- 模型通过 `next_step` 字段获得结构化恢复建议，减少无效重试
- 未来的 tool retry 机制（不在 v0.9 scope）可直接消费 `RetryHint`

**注意**：`RetryHint` 与 v0.7 的 `RetryPolicy` 无直接关系——`RetryPolicy` 处理 **model API 错误**（`ModelError`，HTTP 429/500 等），`RetryHint` 标注 **tool 执行错误** 的重试安全性。两者作用在不同层。

**迁移**：提供便捷构造函数降低迁移成本：

```rust
impl ToolError {
    pub fn fatal(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ErrorKind::Fatal, retry: RetryHint::Unsafe, code: None, next_step: None }
    }
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ErrorKind::InvalidInput, retry: RetryHint::Safe, code: None, next_step: None }
    }
    pub fn transient(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ErrorKind::Transient, retry: RetryHint::Safe, code: None, next_step: None }
    }
    pub fn with_code(mut self, code: impl Into<String>) -> Self { self.code = Some(code.into()); self }
    pub fn with_next_step(mut self, hint: impl Into<String>) -> Self { self.next_step = Some(hint.into()); self }
}
```

现有 `ToolError { message: "...".into(), code: None }` 迁移为 `ToolError::fatal("...")`。有 `code` 的场景用 `.with_code("...")`。

**Display format**：当前 `#[error("{message}")]` 只展示 message。新增字段后需要决定 `Display` 格式——保持只展示 `message`（简洁，向后兼容日志格式）还是改为包含 `kind`（调试友好）。具体在 issue spec 阶段定。

**影响范围**：`ToolError` 结构体、所有 `Tool` 实现者（built-in、MCP bridge、AgentAsTool、SDK）、`ToolCallFailed` 事件（改为携带 `ToolError` 而非 `String`）。

#### 1C. `as_tool()` Builder 模式

**当前**：`AgentConfig::as_tool()` 有 7 个参数（含 2 个闭包），代码注释已标注 `// a builder is planned for v0.8`。

**改为**：

```rust
impl AgentConfig {
    pub fn as_tool(&self, name: &str, description: &str) -> SubAgentBuilder;
}

pub struct SubAgentBuilder { .. }

impl SubAgentBuilder {
    pub fn model(self, model: Arc<dyn ModelAdapter>) -> Self;
    pub fn registry(self, registry: ToolRegistry) -> Self;
    pub fn inherit_context(self, recent_messages: usize) -> Self;
    pub fn inherit_budget(self, parent_budget: &BudgetGuard) -> Self;
    pub fn input_schema(self, schema: Value) -> Self;
    pub fn input_mapper(self, f: impl Fn(Value) -> Result<String, ToolError> + Send + Sync + 'static) -> Self;
    pub fn output_extractor(self, f: impl Fn(Value) -> Value + Send + Sync + 'static) -> Self;
    pub fn build(self) -> Arc<dyn Tool>;
}
```

**`inherit_context` 实现机制**：

`inherit_context(n)` 是延迟绑定——builder 只记录"要继承最近 N 条消息"，实际注入发生在 `AgentAsTool::call()` 执行时：

1. `ToolContext` 新增 `parent_messages: Vec<Message>` 字段
2. `AgentAsTool::call()` 读取 `ctx.parent_messages`，取最后 N 条，prepend 到子 agent 的初始 messages
3. 不调用 `inherit_context` 时 `parent_messages` 被忽略（与当前行为一致：fresh start）

**性能注意**：`state.messages` 可能很大，每次 tool call 都克隆全量 messages 浪费。run loop 应仅在调用 `AgentAsTool` 类型的 tool 时才填充 `parent_messages`（可通过 `ToolMetadata` 上的标记或 tool 类型判断），其他 tool 填 empty vec。具体策略在 issue spec 阶段定。

```rust
// ToolContext 扩展
pub struct ToolContext {
    // ... existing fields ...
    /// Parent run's message history at the time of this tool call.
    /// Populated by the run loop; used by AgentAsTool for context inheritance.
    pub parent_messages: Vec<Message>,
}

// AgentAsTool::call() 中
if let Some(n) = self.inherit_context_count {
    let recent = ctx.parent_messages.iter().rev().take(n).rev().cloned().collect::<Vec<_>>();
    initial_messages.splice(0..0, recent);
}
```

**后向兼容**：旧 7 参数 `as_tool()` 标记 `#[deprecated]`，内部转为 builder 调用，保留一个版本周期。

**影响范围**：`AgentConfig`、`AgentAsTool`、`ToolContext`（新增 `parent_messages`）、run loop 中构造 `ToolContext` 的位置、所有使用 `as_tool()` 的 example 和 test。

#### 1D. `ToolCallStarted` 事件携带 `ToolMetadata`

**当前**：`ToolCallStarted { tool: String, source: ToolSource, input: Value }`——消费者需要回查 registry 才知道 tool 的审批级别、side effect 等信息。

**改为**：

```rust
ToolCallStarted {
    tool: String,
    metadata: ToolMetadata,  // 完整快照，包含 approval、side_effect、cost_hint 等
    input: Value,
}
```

**SD 收益**：`LlmWatcher` 评估事件时直接从事件中读取 tool metadata，不需要持有 registry 引用。

**影响范围**：`RuntimeEvent` 枚举、`actor.rs` 中 emit `ToolCallStarted` 的位置、所有 pattern match `ToolCallStarted` 的代码。`source` 字段已包含在 `ToolMetadata` 中，不再单独列出。

---

### Phase 2：Supervised Delegation 核心

#### 2A. Mid-run Steering API

v0.8 已铺好内部通路，v0.9 完成最后一英里：

**需要做的：**
- `RunHandle` 新增 `pub async fn inject_message(&self, msg: &str)` — 代理到 `AgentMsg::Inject`
- `RunHandle` 新增 `pub async fn steer(&self, instruction: &str)` — 代理到 `AgentMsg::Steer`
- `SteerCmd` 填充 `instruction: String` 字段（当前为空 struct）
- `AgentMsg::Steer` handler 实现：将 instruction 作为 system-role 消息注入 `state.messages`（区别于 Inject 的 user-role）
- `WatcherAction` 新增 `Steer(String)` 变体，与 `Inject(String)` 区分语义：
  - `Inject` → user-role 消息，对模型呈现为用户输入
  - `Steer` → system-role 指令，对模型呈现为系统级重定向

**不需要做的：**
- 消息持久化（已通过 `state.messages` → `SessionSnapshot` 自动覆盖）
- 事件订阅基础（`subscribe_events` / `attach_watcher` 已就绪）

#### 2B. LlmWatcher

`Watcher` trait 的 LLM-powered 实现——用另一个模型监控 worker 事件流，自主判断是否干预：

```rust
pub struct LlmWatcher {
    model: Arc<dyn ModelAdapter>,
    system_prompt: String,
    event_buffer: Mutex<Vec<RuntimeEvent>>,
    eval_interval: usize,  // 每 N 个事件触发一次 LLM 评估
}

#[async_trait]
impl Watcher for LlmWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        // 累积事件到 buffer
        // 达到 eval_interval 时调用 watcher LLM 评估
        // LLM 输出结构化判断 → 映射为 Continue / Inject / Steer / Abort
    }
}
```

**设计要点：**
- 事件累积 + 批量评估，避免每个事件都调用 LLM（成本 + 延迟）
- watcher LLM 的 system prompt 描述监控职责和干预标准
- LLM 输出用 tool_use 结构化（action + reason），不依赖自由文本解析
- watcher 自身的 LLM 调用不经过 run loop，直接使用 `ModelAdapter::chat()`
- 评估时可直接读取 `ToolCallStarted.metadata`（1D）和 `ToolError.kind`（1B）做结构化判断

#### 2C. 崩溃恢复（Ractor Supervision）

**当前架构**：`AgentRun::start()` 直接 `Actor::spawn(WorkerActor, ...)` 得到 `ActorRef<AgentMsg>`。没有 supervisor actor。Worker panic 时 Ractor 默认行为是 actor 终止，无自动恢复。

**目标架构**：引入 `SupervisorActor`，由它 spawn 并监控 `WorkerActor`。supervisor 持有 `SessionStore` 引用，在 worker 失败时根据策略决定 restart 或 stop。

```
AgentRun::start()
  └── spawn SupervisorActor
        └── spawn WorkerActor (linked as child)
              ├── normal exit → supervisor 收到 ActorTerminated → 正常结束
              └── panic/error → supervisor 收到 ActorFailed → 检查策略 → restart or stop
```

**SupervisorActor 设计**：

```rust
pub(crate) struct SupervisorActor;

pub(crate) struct SupervisorState {
    strategy: SupervisionStrategy,
    attempts: u32,
    store: Option<Arc<dyn SessionStore>>,
    worker_args_template: AgentRunArgs,  // 用于 restart 时重建 worker
    event_subs: Vec<mpsc::Sender<RuntimeEvent>>,
    /// Watcher 持久注册表——restart 后重新 attach，保证 watcher 不因 worker 重建而断连。
    watchers: Vec<Arc<dyn Watcher>>,
    /// Shared actor_ref，与 RunHandle 共享同一个 Arc，restart 后更新指向新 worker。
    actor_ref_shared: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
}

pub enum SupervisionStrategy {
    /// Restart from last snapshot, up to max_retries.
    Restart { max_retries: u32 },
    /// Stop and emit RunAborted (default).
    Stop,
}

impl Actor for SupervisorActor {
    type Msg = SupervisorMsg;
    type State = SupervisorState;
    // ...

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<SupervisorMsg>,
        message: SupervisionEvent,
        state: &mut SupervisorState,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorFailed(who, err) => {
                match &state.strategy {
                    SupervisionStrategy::Stop => {
                        emit(&state.event_subs, RuntimeEvent::RunAborted {
                            reason: Some(format!("worker failed: {err}")),
                        }).await;
                        myself.stop(None);
                    }
                    SupervisionStrategy::Restart { max_retries } => {
                        if state.attempts >= *max_retries {
                            emit(&state.event_subs, RuntimeEvent::RunAborted {
                                reason: Some(format!("worker failed after {} restarts: {err}", state.attempts)),
                            }).await;
                            myself.stop(None);
                            return Ok(());
                        }
                        state.attempts += 1;
                        // Load latest snapshot and rebuild worker
                        let snapshot = load_latest_snapshot(&state.store, &state.worker_args_template).await;
                        let new_args = rebuild_args_from_snapshot(snapshot, &state.worker_args_template);
                        let (worker_ref, _) = Actor::spawn_linked(
                            None, WorkerActor, new_args, myself.get_cell(),
                        ).await?;
                        // Update shared actor_ref so RunHandle points to new worker
                        if let Ok(mut guard) = state.actor_ref_shared.lock() {
                            *guard = Some(worker_ref.clone());
                        }
                        // Re-attach all watchers to new worker's event stream
                        for watcher in &state.watchers {
                            reattach_watcher(&worker_ref, Arc::clone(watcher)).await;
                        }
                        emit(&state.event_subs, RuntimeEvent::RunRestarted {
                            attempt: state.attempts,
                        }).await;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(_, _, _) => {
                // Normal exit — propagate stop
                myself.stop(None);
            }
            _ => {}
        }
        Ok(())
    }
}
```

**关键细节：**
- `AgentRun::start()` 改为先 spawn `SupervisorActor`，supervisor 再 `spawn_linked` `WorkerActor`
- `RunHandle.actor_ref` 仍指向 `WorkerActor`（steering/inject 直接发给 worker），supervisor 持有同一个 `Arc<Mutex<Option<ActorRef>>>` 以便 restart 后更新引用
- `AgentConfig` 新增 `supervision_strategy` 字段（默认 `SupervisionStrategy::Stop`，与当前行为一致）
- 每次 restart 后 emit `RuntimeEvent::RunRestarted { attempt }` — watcher 可观察此事件并做出反应
- `SupervisorActor` 不处理 `AgentMsg`，只处理 supervision events + 少量管理消息（如 shutdown）

**Watcher 跨 restart 存活：**

Worker restart 会导致旧 actor drop、event channel closed，之前 attach 的 watcher task 会因 `rx.recv() == None` 退出。必须在 restart 后重新连接 watcher。

设计：
- `RunHandle::attach_watcher()` 改为双写——除了 spawn watcher task，还通过 supervisor 消息注册 watcher 到 `SupervisorState.watchers`
- supervisor restart worker 后，遍历 `watchers` 列表，对每个 watcher 重新 `subscribe_events` + spawn 新的消费 task
- watcher 注册是持久的（跟随 supervisor 生命周期），不是跟随单个 worker 实例

**前置条件**：`SessionPersistenceHook` 在 `on_run_error` 时已保存 snapshot（v0.8 已实现）。

#### 2D. 多 Watcher 协调

v0.8 已支持多个 subscriber 和多次 `attach_watcher`。v0.9 明确协调语义：

- **FIFO**：多个 watcher 的 `Inject` / `Steer` 按到达时间序处理，不做合并
- **Abort 优先**：任一 watcher 发出 `Abort` → 立即终止 run，其他 watcher 的后续动作丢弃
- **无优先级排序**：v0.9 不引入 watcher 优先级机制（过度设计风险）

这意味着当前 `attach_watcher` 的实现基本够用，只需确保 Abort 的 cancel 语义正确传播到所有 watcher 的事件 receiver。

---

### Phase 3：端到端示例 + 验证

一个完整的 Supervised Delegation 示例：

```
examples/rust/supervised_delegation.rs
```

流程：
1. 启动 worker agent（配置 `SupervisionStrategy::Restart { max_retries: 2 }`）
2. 注册 LlmWatcher（或 mock watcher 用于测试）
3. worker 执行 tool call → emit event → watcher 评估
4. 演示 Inject（注入用户消息）、Steer（系统级重定向）、Abort（终止）三种干预
5. 演示 worker panic → supervisor 收到 `ActorFailed` → restart from snapshot

对应集成测试覆盖上述 5 个场景。

## 不在范围内

- Provider 扩展（→ [v0.9.1](../../iteration/v0_9_1/prd.md)）
- 文档、发布准备（→ [v0.9.2](../../iteration/v0_9_2/prd.md)）
- 分布式 agent 编排（多进程 / 多机）
- 内置 observability 平台
- Watcher 优先级排序 / Steer 合并层
- Tool retry 机制（`RetryHint` 提供信息，但 v0.9 不实现自动 tool 重试）
- Skill marketplace / Web UI

## 依赖

- v0.8 Session 持久化（崩溃恢复需要 snapshot）
- v0.8 Watcher trait + attach_watcher（SD 基础通信层）
- v0.8 AgentMsg::Inject / Steer 消息枚举（内部通路已就绪）
- [外部研究综合分析](../../analysis/external-research-synthesis.md)（Phase 1 API 改造的设计来源）

## 验收标准

### Phase 1：API 改造
- [ ] `ToolMetadata.approval` 为 `Approval` 枚举（`Never / WhenRisky / Always`），`requires_approval: bool` 已移除
- [ ] `ApprovalMode::SideEffectOnly` 标记 `#[deprecated]`，SDK 解析保留但发出 warning
- [ ] `RuntimeConfig::should_approve()` 基于 `Approval` 枚举分派
- [ ] `ToolError` 包含 `kind: ErrorKind`、`retry: RetryHint`、`code: Option<String>`、`next_step: Option<String>`
- [ ] `ToolError` 提供 `fatal()` / `invalid_input()` / `transient()` 便捷构造 + `with_code()` / `with_next_step()` 链式方法
- [ ] 所有现有 `Tool` 实现者已迁移到新 `ToolError` 构造
- [ ] `ToolCallFailed` 事件携带 `ToolError`（替代 `error: String`）
- [ ] `AgentConfig::as_tool()` 返回 `SubAgentBuilder`，支持 `inherit_context(n)`
- [ ] `ToolContext` 新增 `parent_messages` 字段，run loop 填充
- [ ] 旧 7 参数 `as_tool()` 标记 `#[deprecated]`
- [ ] `ToolCallStarted` 事件携带 `ToolMetadata` 完整快照

### Phase 2：SD 核心
- [ ] `RunHandle::inject_message()` 可用，注入 user-role 消息，模型下一轮迭代时响应
- [ ] `RunHandle::steer()` 可用，注入 system-role 指令，模型下一轮迭代时响应
- [ ] `WatcherAction` 枚举包含 `Continue / Inject / Steer / Abort` 四个变体
- [ ] `LlmWatcher` 可用——累积事件、批量评估、结构化输出映射
- [ ] `SupervisorActor` 引入，`AgentRun::start()` 通过 supervisor 间接 spawn worker
- [ ] 崩溃恢复可用——worker panic 后 supervisor 根据 `SupervisionStrategy` restart from snapshot
- [ ] `AgentConfig` 支持 `supervision_strategy` 字段（默认 `Stop`）
- [ ] Watcher 跨 restart 存活——worker restart 后已注册的 watcher 自动重新 attach
- [ ] 多 watcher FIFO 协调——Inject/Steer 按序到达，Abort 立即生效

### Phase 3：验证
- [ ] `examples/rust/supervised_delegation.rs` 端到端可运行
- [ ] 集成测试覆盖：inject、steer、abort、panic-restart、multi-watcher 五个场景
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
