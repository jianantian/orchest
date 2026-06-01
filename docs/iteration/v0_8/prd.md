# v0.8 Spec：持久化 + 安全 + Supervised Delegation 基础

## 背景

v0.7 建立了 Hook 框架、Handoff 两层语义，并通过 Ractor PoC 决定了 runtime 的通信架构走向。runtime 具备了扩展能力，但仍缺少产品级 agent 应用的关键能力：

1. **Session 持久化**：当前 run 的状态纯内存，进程退出即丢失。长时间运行的 agent、多轮对话场景无法恢复。竞品对比标记为 🔴 级缺口
2. **安全层**：`requires_approval: bool` 是唯一的安全机制。产品级应用需要 guardrail 和多模式权限控制
3. **Supervised Delegation 基础通信层**：[Actor Model 评估](../../research/actor-model-evaluation.md) 识别的核心验证 case，需要双向通信、多方事件订阅、watcher 注册等基础能力。这些基础在 v0.8 建好后，v0.9 的 Mid-run Steering 和 Supervised Delegation 完整实现就变成"已有能力的 API 暴露"而不是"从零搭建通信层"

## 目标

1. 提供可插拔的 `SessionStore` trait，支持 run 状态的持久化和恢复
2. 提供 Guardrail 框架（Input / Output / ToolInput / ToolOutput 四层），作为 Hook 的便利封装
3. 权限模型从 per-tool 布尔值扩展为 run 级 `ApprovalMode` 策略
4. 建立 Supervised Delegation 所需的双向通信和多方事件订阅基础

## 范围

---

### Session 持久化

#### `SessionStore` trait

```rust
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError>;
    async fn load(&self, session_id: &str) -> Result<Option<SessionSnapshot>, SessionError>;
    async fn delete(&self, session_id: &str) -> Result<(), SessionError>;
    async fn list(&self) -> Result<Vec<String>, SessionError>;
}
```

#### `SessionSnapshot` 定义

`SessionSnapshot` 是 `RunState` 的可序列化投影，**不等同于也不派生自 `RunState`**（`RunState` 含不可序列化字段如 `available_tools`）。字段：

```rust
#[derive(Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: String,          // 从 RunState::schema_version 同字段迁移
    pub session_id: String,
    pub run_id: RunId,
    pub messages: Vec<Message>,
    pub step: u32,
    pub budget_used: BudgetUsage,
    pub active_config: AgentConfig,      // 记录 handoff 后当前生效的 agent 配置
}
```

**设计说明：**
- `active_config`：handoff 会切换 config/registry，snapshot 必须记录切换后的配置；若未发生 handoff，等于初始 config。`AgentConfig` 的 non-serializable 字段（hooks、retry_policy、handoffs）已标 `#[serde(skip)]`，恢复时由调用方重新注册
- **显式排除**：`available_tools`（运行时从 config 重建）、async job 在途状态（poll 闭包不可序列化；进行中的 async job 在恢复后视为已超时或丢弃，用户应监听 `AsyncToolCompleted` 并在 resume 前等待结束）
- **schema_version**：当前值 `"0.1"`，snapshot 格式破坏性变更时递增；`load` 遇到不匹配版本时返回 `SessionError::SchemaMismatch`

#### resume API

```rust
impl AgentRun {
    /// Resume a previous run from a snapshot.
    /// `model` and `registry` are reconstructed by the caller from `snapshot.active_config`.
    pub fn resume(
        snapshot: SessionSnapshot,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver);
}
```

`resume` 与 `start` 的区别：用 `snapshot.messages` 作为初始历史（而非从用户 input 构建），用 `snapshot.run_id` 保持一致性，从 `snapshot.step` 继续计步。

#### 保存触发点

通过 hook 框架的 `on_run_end` 和 `on_run_error` **两条路径**都触发自动保存（仅 `on_run_end` 无法覆盖崩溃前的最后状态）。保存由 `SessionPersistenceHook` 实现，注册为普通 Hook：

```rust
pub struct SessionPersistenceHook {
    pub store: Arc<dyn SessionStore>,
    pub session_id: String,
}
```

step 级 checkpoint（每步保存）推迟到 v0.9。

#### 内置实现与依赖策略

| 实现 | crate/feature | 依赖 |
|------|---------------|------|
| `InMemorySessionStore` | `agent-runtime-core`（默认） | 无新依赖 |
| `SqliteSessionStore` | `agent-runtime-core` feature `sqlite-session` | `rusqlite`（bundled feature，静态链接，无外部系统依赖） |

选 `rusqlite` 而非 sqlx：schema 极简（单表存 JSON），不需要 async driver 或 migration 框架。`bundled` feature 避免依赖系统 libsqlite3，对 PyO3/napi 构建友好。`agent-runtime-py` 和 `agent-runtime-node` 的 Cargo.toml 不强制开启此 feature。

竞品参考：OpenAI `Session`（SQLite / Server 后端）、pi-agent `SessionStore`（可插拔后端）、Claude SDK JSONL 持久化。

---

### Guardrails

#### 与 Hook trait 的关系

Guardrail 是 Hook 的**便利封装层**，底层注册为 Hook 实现，不修改 Hook trait 的结构——但需要两处最小扩展以支持"拒绝并回传"语义：

**Hook 契约变更（v0.8 引入）：**

1. `HookAction` 新增 `Reject(String)` 变体：
   ```rust
   pub enum HookAction {
       Continue,
       Skip,      // 保持现有语义（跳过工具，返回 "skipped by hook"）
       Abort(String),
       Reject(String),  // 新增：拒绝当前工具调用，以 reason 作为 tool result 回传给模型
   }
   ```
   `Reject` 与 `Skip` 的区别：`Skip` 回传固定字符串，`Reject` 回传用户指定的 reason，语义上是"拒绝并告知模型原因，让模型换策略"。

2. `ToolHookContext` 新增 `tool_output` 字段：
   ```rust
   pub struct ToolHookContext {
       pub run_id: RunId,
       pub tool_name: String,
       pub tool_input: serde_json::Value,
       pub tool_metadata: ToolMetadata,
       pub tool_output: Option<serde_json::Value>,  // 新增：after_tool 阶段填充
   }
   ```
   `before_tool` 调用时为 `None`；`after_tool` 调用时为工具实际输出，hook 可修改后写回，run loop 用修改后的值替换 tool_result。

Run loop 中 `Reject` 的处理：不调用工具执行，将 reason 作为 tool error 写入 tool_results 并继续循环（与 approval denied 路径对称，但原因由 hook 提供）。

#### 四层 Guardrail

| 层 | 对应 hook 点 | 可用操作 |
|---|---|---|
| `InputGuardrail` | `before_model` | Allow / Replace(Vec\<Message\>) / Abort(reason) |
| `OutputGuardrail` | `after_model` | Allow / Replace(response_content: Value) / Abort(reason) |
| `ToolInputGuardrail` | `before_tool` | Allow / Modify(new_input: Value) / Reject(reason) / Abort(reason) |
| `ToolOutputGuardrail` | `after_tool` | Allow / Modify(new_output: Value) / Abort(reason) |

**语义实现映射：**
- `InputGuardrail::Replace(msgs)` → 写入 `ctx.messages`，返回 `ModelHookAction::Continue`（`before_model` 已有 `&mut ModelHookContext`）
- `OutputGuardrail::Replace(content)` → 修改 `ctx.messages` 最后一条 assistant message，返回 `HookAction::Continue`
- `ToolInputGuardrail::Modify(input)` → 写入 `ctx.tool_input`，返回 `HookAction::Continue`（`before_tool` 已有 `&mut ToolHookContext`，且 loop 已用 `tool_hook_ctx.tool_input` 作为实际调用输入）
- `ToolInputGuardrail::Reject(reason)` → 返回 `HookAction::Reject(reason)`
- `ToolOutputGuardrail::Modify(output)` → 写入 `ctx.tool_output`，返回 `HookAction::Continue`

#### 用户 API

用户可以：
1. **直接实现 Hook trait**（最灵活）
2. **实现 `Guardrail` trait** 并调用 `AgentConfig::with_guardrail(...)` 注册（便利路径，框架负责 Hook 适配）

```rust
// 示例：屏蔽特定关键词的 ToolInputGuardrail
let guardrail = KeywordBlockGuardrail::new(vec!["rm -rf", "DROP TABLE"]);
let config = AgentConfig::builder("model")
    .with_guardrail(Arc::new(guardrail))
    .build()?;
```

Core 只提供框架；不内置任何具体 guardrail 实现（极简 Core 原则）。

竞品参考：OpenAI 4 层 guardrail（Input / Output / ToolInput / ToolOutput）、Claude SDK `PreToolUse` / `PostToolUse` hook。

---

### 权限模型扩展

#### 语义：ApprovalMode 是 run 级策略，不替代 per-tool flag

当前实现：`ToolMetadata.requires_approval: bool`（per-tool）决定是否触发审批。
v0.8 扩展：`RuntimeConfig` 新增 `approval_mode: ApprovalMode`，作为 run 级**覆盖策略**：

```rust
#[derive(Clone, Serialize, Deserialize, Default)]
pub enum ApprovalMode {
    #[default]
    PerTool,                              // 沿用 tool.metadata().requires_approval（向后兼容默认）
    None,                                 // 永不审批
    All,                                  // 所有工具调用都审批
    SideEffectOnly,                       // side_effect: true 的工具审批（ToolMetadata 已有此字段）
    #[serde(skip)]
    Custom(Arc<dyn Fn(&ToolMetadata) -> bool + Send + Sync>),
}

impl ApprovalMode {
    pub fn should_approve(&self, meta: &ToolMetadata) -> bool {
        match self {
            Self::PerTool => meta.requires_approval,
            Self::None => false,
            Self::All => true,
            Self::SideEffectOnly => meta.side_effect,
            Self::Custom(f) => f(meta),
        }
    }
}
```

Run loop 中原来的 `if tool.metadata().requires_approval {` 替换为 `if state.config.runtime.approval_mode.should_approve(tool.metadata()) {`。

**向后兼容**：`ApprovalMode` 默认为 `PerTool`，行为与当前完全一致；现有代码设置 `tool.metadata().requires_approval = true` 的工具在默认模式下行为不变。

**`Custom` 的 serde 处理**：标 `#[serde(skip)]`，`AgentConfig` 反序列化后 `approval_mode` 恢复为 `PerTool`，与 hooks/retry_policy 处理方式一致。`Custom` 只通过代码路径设置，不支持配置文件序列化。

**与 `ToolGuardrail` 的关系**：`ApprovalMode` 走审批总线（异步等待用户响应）；`ToolInputGuardrail::Reject` 是同步拒绝（不需要用户响应）。两者语义不同，可同时使用：审批通过后 guardrail 仍可拒绝。

竞品参考：Craft Agents 5 种模式（Safe / AcceptEdits / Plan / Ask / Admin）。

---

### Supervised Delegation 基础通信层

#### 多方事件订阅（已有基础，需暴露 API）

v0.7 已在 `AgentRunState` 中实现 `event_subs: Vec<mpsc::Sender<RuntimeEvent>>`，`emit()` 对非主订阅者用 `try_send`（有损投递）。v0.8 只需暴露订阅 API：

```rust
impl RunHandle {
    /// Add a secondary event subscriber.
    /// Delivery to secondary subscribers is lossy: if the channel is full,
    /// the event is dropped and an `EventsDropped` event is sent to the primary.
    pub async fn subscribe_events(&self, capacity: usize) -> EventReceiver;
}
```

`subscribe_events()` 向 actor 发送 `AgentMsg::Subscribe(tx)` 消息，actor 将新 Sender 追加到 `event_subs`。需新增 `AgentMsg::Subscribe` 变体。

**投递保证（显式声明）**：
- **主订阅者**（`AgentRun::start` 返回的 `EventReceiver`）：有损（channel 满时 run loop 阻塞等待；capacity 256 是已知欠妥，v0.8 提取为 const 并评估是否需要提高）
- **次级订阅者**（via `subscribe_events`）：有损，高负载下可能丢失事件

Watcher 实现应将 `WatcherAction` 的触发设计为**容忍事件丢失**。需要精确事件保证的场景（如计费、审计）应使用主订阅者。

broadcast channel 方案在 v0.7 actor 实现时已评估并未选用，PRD 不再保留该备选。

#### 双向通信（内部通道 + InjectCmd payload 落地）

v0.7 已在 `AgentMsg` 中 forward-declare `Inject(InjectCmd, RpcReplyPort<()>)` 和 `Steer(SteerCmd, RpcReplyPort<SteerResult>)`，handler 返回"not yet implemented"。v0.8 实现 `Inject` 路径（`Steer` 保留 stub，完整实现待 v0.9）：

```rust
// v0.7 已存在的空结构体，v0.8 补充 payload
pub struct InjectCmd {
    pub message: String,     // 注入到下一轮 model messages
}
```

Actor 在每轮 `RunStep` 开始前检查 mailbox 中的 `Inject` 消息（actor mailbox FIFO 天然支持），将 `message` 以 user role 消息插入当前 messages 列表，作为模型的下一轮输入。

`Steer` 保留"not yet implemented"占位（v0.9 实现 Mid-run Steering 语义），`pub(crate)` visibility 不变。

#### Watcher 注册

```rust
#[async_trait]
pub trait Watcher: Send + Sync {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction;
}

pub enum WatcherAction {
    Continue,
    Inject(String),   // 注入消息（通过内部 InjectCmd）
    Abort(String),    // 终止 run
}
```

注册 API：

```rust
impl RunHandle {
    pub async fn attach_watcher(&self, watcher: Arc<dyn Watcher>, capacity: usize);
}
```

`attach_watcher` 内部：调用 `subscribe_events(capacity)` 获取事件流，spawn 一个 watcher task（`tokio::spawn`）消费事件流并根据 `WatcherAction` 向 actor 发送 `Inject` 或 `Cancel`。watcher task 的生命周期与 RunHandle 关联，RunHandle drop 时 channel 关闭 watcher task 自然退出。

`WatcherAction::Inject(msg)` → `cast!(actor_ref, AgentMsg::Inject(InjectCmd { message: msg }, reply))` → actor 在下一个 RunStep 消费并插入 messages。

**Steer vs Inject 的区别**：v0.8 只提供 `Inject`（向 messages 注入内容，软干预）；`Steer`（中断当前步骤并强制导航，硬干预）是 v0.9 能力。

---

## 不在范围内

- Step 级 checkpoint（每步持久化，崩溃恢复粒度更细）→ v0.9
- Session 跨进程同步 / 分布式存储（用户可自行实现 `SessionStore`）
- 内置的 LLM-based guardrail（用户基于 guardrail API 自行实现）
- Watcher LLM 实现（watcher 回调中调用另一个模型做判断）→ v0.9
- Mid-run Steering 的 `RunHandle::steer()` / 公共 `inject_message()` API → v0.9（v0.8 的 `Inject` 是内部机制，不作为公共 API 暴露）
- Supervised Delegation 完整实现（watcher 中途干预、崩溃恢复完整流程）→ v0.9
- Provider 扩展 → v0.9
- `AgentMsg::Steer` 实现（保留 stub）→ v0.9

## 依赖

- v0.7 Hook 框架（guardrail 和 session 自动保存都基于 hook 实现）
- v0.7 Handoff 重构（SessionSnapshot.active_config 需正确记录 handoff 后的 agent）
- v0.7 PoC 结论（Ractor 方案，event_subs + ActorRef 已就位）
- **v0.8 内部契约变更**：`HookAction::Reject(String)` 和 `ToolHookContext.tool_output` 是对现有公共类型的扩展，需在拆 issue 时先落地（建议作为 001 的一部分），其他 issue 依赖此变更

## 验收标准

### Session 持久化

- [ ] `SessionStore` trait 定义完整，含 save / load / delete / list
- [ ] `SessionSnapshot` 包含 schema_version、session_id、run_id、messages、step、budget_used、active_config
- [ ] `InMemorySessionStore` 可用（无新依赖）
- [ ] `SqliteSessionStore` 在 `sqlite-session` feature 下可用，可保存和恢复 run 状态
- [ ] `AgentRun::resume(snapshot, model, registry)` API 存在，恢复后的 run 从正确历史和 budget 状态继续
- [ ] `SessionPersistenceHook` 在 `on_run_end` 和 `on_run_error` 两条路径都触发保存
- [ ] 恢复后 handoff 切换过的 agent 身份（active_config）正确还原
- [ ] schema_version 不匹配时 `load` 返回 `SessionError::SchemaMismatch`

### Guardrails

- [ ] `HookAction::Reject(String)` 变体存在，run loop 处理为"拒绝 + reason 作为 tool result"
- [ ] `ToolHookContext.tool_output` 字段存在，after_tool hook 修改后 run loop 使用修改后的值
- [ ] InputGuardrail（before_model）可拦截并替换发给模型的消息列表
- [ ] OutputGuardrail（after_model）可拦截并替换模型输出内容
- [ ] ToolInputGuardrail（before_tool）可 Reject 工具调用，reason 正确回传给模型（而非终止 run）
- [ ] ToolOutputGuardrail（after_tool）可修改工具输出内容
- [ ] Guardrail `Abort` 正确终止 run（与 Hook::Abort 行为一致）
- [ ] 各层有工作示例

### 权限模型

- [ ] `RuntimeConfig.approval_mode: ApprovalMode` 存在
- [ ] `ApprovalMode::PerTool` 为默认值，行为与现有 `requires_approval` 完全一致（向后兼容）
- [ ] `ApprovalMode::None / All / SideEffectOnly` 各自行为正确
- [ ] `ApprovalMode::Custom(f)` 可用，`f` 可访问完整 `ToolMetadata`
- [ ] `Custom` 标 `#[serde(skip)]`，反序列化后退化为 `PerTool`

### Supervised Delegation 基础

- [ ] `RunHandle::subscribe_events(capacity)` 存在，返回次级 EventReceiver
- [ ] 多订阅者并发消费同一 run 的事件流（主 + 至少一个次级）
- [ ] 次级订阅者满时发出 `EventsDropped` 事件，不阻塞 run loop
- [ ] `InjectCmd.message: String` 字段存在，actor 在下一 RunStep 将消息插入 messages
- [ ] `Watcher` trait 定义完整（`#[async_trait]`），含 `on_event` 方法
- [ ] `WatcherAction::Inject(msg)` 使用内部 `InjectCmd` 向 run loop 注入消息，下一轮 model call 可见
- [ ] `WatcherAction::Abort(reason)` 正确终止 run
- [ ] `RunHandle::attach_watcher(watcher, capacity)` 存在，watcher 可接收 run 事件

### 通用

- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
- [ ] sqlite-session feature 下构建不破坏 agent-runtime-py / agent-runtime-node 的默认构建（feature 不 propagate）
