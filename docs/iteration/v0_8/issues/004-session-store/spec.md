# 004 · SessionStore + InMemorySessionStore + resume API

## 背景

当前 run 状态（messages、budget、step）完全在内存中，进程退出即丢失。`RunState`（`run/config.rs`）虽然含 schema_version / messages / budget_used / step，但同时包含 `#[serde(skip)]` 的 `available_tools`，且没有 session 身份（session_id）。

本 issue 定义持久化契约：`SessionStore` trait、`SessionSnapshot` 类型、内置 `InMemorySessionStore`（默认）、`SessionPersistenceHook`（自动保存）、`AgentRun::resume()` 恢复入口。SQLite 实现留 005。

本 issue 独立于 001/002/003，可并行开发。

## 目标

让 agent run 的状态可以被保存和恢复；Session 持久化是可选的，不改变默认内存行为。

## 范围

### 模块结构

```
crates/agent-runtime-core/src/session/
├── mod.rs              # 声明子模块，re-export 公共类型
├── store.rs            # SessionStore trait + SessionError + InMemorySessionStore
├── snapshot.rs         # SessionSnapshot 类型
└── persistence_hook.rs # SessionPersistenceHook（实现 Hook）
```

### SessionSnapshot

```rust
// session/snapshot.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: String,        // 当前："0.1"
    pub session_id: String,
    pub run_id: crate::run::RunId,
    pub messages: Vec<crate::model::Message>,
    pub step: u32,
    pub budget_used: crate::budget::BudgetUsage,
    pub active_config: crate::run::AgentConfig,  // handoff 后当前生效的 agent 配置
}

impl SessionSnapshot {
    pub const CURRENT_SCHEMA_VERSION: &'static str = "0.1";
}
```

`active_config` 中的 `#[serde(skip)]` 字段（hooks、retry_policy、handoffs）在反序列化后为空；调用方在 `resume` 时通过 `model` 和 `registry` 参数重建运行时资源。

**不含以下字段（显式排除）：**
- `available_tools`：运行时从 `active_config` 和 `registry` 重建
- async job 在途状态：poll 闭包不可序列化；进行中的 async job 在 resume 后视为已超时

### SessionStore trait

```rust
// session/store.rs

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError>;
    async fn load(&self, session_id: &str) -> Result<Option<SessionSnapshot>, SessionError>;
    async fn delete(&self, session_id: &str) -> Result<(), SessionError>;
    async fn list(&self) -> Result<Vec<String>, SessionError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("schema version mismatch: expected {expected}, got {found}")]
    SchemaMismatch { expected: String, found: String },
    #[error("session not found: {0}")]
    NotFound(String),
}
```

### InMemorySessionStore

```rust
// session/store.rs（同文件）

#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: Arc<tokio::sync::Mutex<HashMap<String, SessionSnapshot>>>,
}
```

`save` 序列化为 JSON 再反序列化，确保 round-trip 语义与 SQLite 实现一致（避免引用共享导致测试与生产行为不同）。

`load` 检查 `schema_version`，不匹配时返回 `SessionError::SchemaMismatch`。

### SessionPersistenceHook

```rust
// session/persistence_hook.rs

pub struct SessionPersistenceHook {
    pub store: Arc<dyn SessionStore>,
    pub session_id: String,
}
```

实现 `Hook::on_run_end` 和 `Hook::on_run_error`（两条路径都保存），在保存前从当前运行状态构建 `SessionSnapshot`。

**问题**：`on_run_end` 和 `on_run_error` 的 `ctx: &RunHookContext` 只含 `run_id / agent_name / step`，**不含** messages 和 budget_used。Hook 无法访问完整运行状态。

**解决方案**：`SessionPersistenceHook` 内部持有一个 `Arc<Mutex<Option<SessionSnapshot>>>`，通过 `before_model` hook 点（每轮 model call 前）更新快照（此时 messages 和 step 信息在 `ModelHookContext` 中）。`on_run_end` / `on_run_error` 时，保存最后一次 `before_model` 时记录的快照。

这意味着快照最多落后一个 step，但能覆盖崩溃恢复场景（crash 发生在 model call 和下一个 `on_run_end` 之间时，恢复到上一轮 model call 之前的状态）。

```rust
impl SessionPersistenceHook {
    // 内部持有最新快照
    snapshot_cache: Arc<Mutex<Option<SessionSnapshot>>>,
}
```

**注意**：`before_model` 没有 `budget_used`。可将 `BudgetUsage` 的 update 也通过 `after_model` 近似（after_model 的 ctx 也只有 messages，没有 budget）。最简方案：`RunHookContext` 在 v0.8 扩展，增加 `budget_used: BudgetUsage` 字段（on_run_end 时 run loop 可以填入）。这样 `on_run_end` / `on_run_error` 的 ctx 包含最终预算信息，hook 可直接构建 snapshot。

**最终方案**：v0.8 中扩展 `RunHookContext`，新增 `budget_used: BudgetUsage` 字段，run loop 在调用 `on_run_end` / `on_run_error` 前填入当前预算使用量。同时在 `on_run_end`/`on_run_error` 的 ctx 中也需要 messages——再新增 `final_messages: Vec<Message>` 字段。两个字段共同构成完整 snapshot。

```rust
pub struct RunHookContext {
    pub run_id: RunId,
    pub agent_name: String,
    pub step: u32,
    // v0.8 新增（仅在 on_run_end / on_run_error 调用时有意义；on_run_start 时为空/零值）
    pub budget_used: crate::budget::BudgetUsage,
    pub final_messages: Vec<crate::model::Message>,
    pub active_config: Option<crate::run::AgentConfig>,  // handoff 后当前 agent，无 handoff 时为 None
}
```

`SessionPersistenceHook::on_run_end` / `on_run_error` 直接从 `ctx` 构建 snapshot 并调用 `store.save`。

### AgentConfig 新增 session 字段

```rust
pub struct AgentConfig {
    // ... 现有字段 ...
    #[serde(skip)]
    pub session_store: Option<Arc<dyn SessionStore>>,
    pub session_id: Option<String>,
}
```

若 `session_store` 非空，`AgentRun::start` 自动注册 `SessionPersistenceHook`（用户也可手动注册，但自动注册更便利）。

### AgentRun::resume API

```rust
impl AgentRun {
    /// Resume a previous run from a persisted snapshot.
    pub fn resume(
        snapshot: SessionSnapshot,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver);
}
```

`resume` 与 `start` 的区别：
- 不从 `input` 构建初始 user message；直接用 `snapshot.messages` 作为初始历史
- 用 `snapshot.run_id` 保持 RunId 一致性（event stream 中 run_id 连续）
- 从 `snapshot.step` 继续计步
- 从 `snapshot.budget_used` 恢复预算状态
- 用 `snapshot.active_config` 作为 AgentConfig（hooks/retry_policy/handoffs 需调用方在 `active_config` 上重新注册后传入，或在 `registry` 中预配置）

## 验收标准

- [ ] `SessionSnapshot` 类型定义完整，含全部字段，可 JSON 序列化/反序列化
- [ ] `SessionStore` trait 定义完整，含 save/load/delete/list
- [ ] `SessionError::SchemaMismatch` 在版本不匹配时返回
- [ ] `InMemorySessionStore` round-trip 语义正确（save 后 load 得到等价 snapshot）
- [ ] `RunHookContext` 新增 `budget_used` / `final_messages` / `active_config` 字段，run loop 在 on_run_end / on_run_error 前填入
- [ ] `SessionPersistenceHook` 在 `on_run_end` 和 `on_run_error` 两条路径都保存 snapshot
- [ ] `AgentConfig.session_store` / `session_id` 字段存在；设置后 `AgentRun::start` 自动注册 hook
- [ ] `AgentRun::resume(snapshot, model, registry)` 存在，恢复后的 run 从正确历史和 budget 状态继续
- [ ] resume 后的 run_id 与 snapshot 中一致
- [ ] resume 后 step 计数从 snapshot.step 继续
- [ ] handoff 后的 active_config 正确恢复（resume 后用正确的 agent 配置）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `RunHookContext` 是公共类型（py/node binding 可能引用），新增字段需检查 binding crates 是否受影响
- `final_messages` 在 `on_run_start` 时为空 vec，在 `on_run_error` 时可能不包含最后一次 model 响应（取决于 crash 发生在哪个阶段）；这是可接受的近似
- `active_config` 在无 handoff 时为 `None`；hook 中判断 `None` 时使用 `AgentRun::start` 时的 original config 作为 snapshot.active_config（需要在 `SessionPersistenceHook` 创建时记录 original config）
