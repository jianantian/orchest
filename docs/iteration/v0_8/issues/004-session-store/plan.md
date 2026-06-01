# 004 · SessionStore + InMemory + resume — 实施计划

## 前置条件

- `cargo test --workspace` 全绿（基线确认）
- 本 issue 不依赖 001/002/003

---

## 步骤

### 步骤 1：`hook/mod.rs` — 扩展 RunHookContext

新增三个字段（v0.8 需要，向后兼容：字段有零值默认）：

```rust
pub struct RunHookContext {
    pub run_id: RunId,
    pub agent_name: String,
    pub step: u32,
    // v0.8 新增：仅 on_run_end / on_run_error 时有意义
    pub budget_used: crate::budget::BudgetUsage,
    pub final_messages: Vec<crate::model::Message>,
    pub active_config: Option<crate::run::AgentConfig>,
}
```

`RunHookContext` 构造点（`actor.rs`）新增字段填 `Default`：
```rust
let mut run_hook_ctx = RunHookContext {
    run_id,
    agent_name: ...,
    step: 0,
    budget_used: BudgetUsage::default(),
    final_messages: vec![],
    active_config: None,
};
```

在 `actor.rs` 的 `on_run_end` / `on_run_error` 调用前填充：
```rust
state.run_hook_ctx.budget_used = state.budget.usage().clone();
state.run_hook_ctx.final_messages = state.messages.clone();
state.run_hook_ctx.active_config = Some(state.config.clone());
// 然后调用 run_on_run_end / run_on_run_error
```

### 步骤 2：新建 `src/session/` 模块

**`session/snapshot.rs`**：定义 `SessionSnapshot` 结构体（见 spec）。

**`session/store.rs`**：定义 `SessionStore` trait、`SessionError`、`InMemorySessionStore`。

`InMemorySessionStore::save` 内部做 JSON round-trip：
```rust
let json = serde_json::to_string(snapshot)?;
let parsed: SessionSnapshot = serde_json::from_str(&json)?;
self.sessions.lock().await.insert(session_id.to_string(), parsed);
Ok(())
```

`InMemorySessionStore::load` 检查 schema_version：
```rust
if snap.schema_version != SessionSnapshot::CURRENT_SCHEMA_VERSION {
    return Err(SessionError::SchemaMismatch { ... });
}
```

**`session/persistence_hook.rs`**：

```rust
pub struct SessionPersistenceHook {
    pub store: Arc<dyn SessionStore>,
    pub session_id: String,
    original_config: AgentConfig,  // start 时的初始 config，用于无 handoff 情况
}

impl SessionPersistenceHook {
    fn build_snapshot(&self, ctx: &RunHookContext) -> SessionSnapshot {
        SessionSnapshot {
            schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
            session_id: self.session_id.clone(),
            run_id: ctx.run_id,
            messages: ctx.final_messages.clone(),
            step: ctx.step,
            budget_used: ctx.budget_used.clone(),
            active_config: ctx.active_config.clone().unwrap_or(self.original_config.clone()),
        }
    }
}

#[async_trait]
impl Hook for SessionPersistenceHook {
    async fn on_run_end(&self, ctx: &RunHookContext) {
        let snap = self.build_snapshot(ctx);
        // fire-and-forget; log error on failure
        if let Err(e) = self.store.save(&self.session_id, &snap).await {
            tracing::error!("SessionPersistenceHook: save failed: {e}");
        }
    }
    async fn on_run_error(&self, ctx: &RunHookContext, _error: &str) {
        let snap = self.build_snapshot(ctx);
        if let Err(e) = self.store.save(&self.session_id, &snap).await {
            tracing::error!("SessionPersistenceHook: save on error failed: {e}");
        }
    }
}
```

**`session/mod.rs`**：声明子模块，re-export 公共类型。

### 步骤 3：`run/config.rs` — AgentConfig session 字段

```rust
pub struct AgentConfig {
    // ... 现有字段 ...
    #[serde(skip)]
    pub session_store: Option<Arc<dyn crate::session::SessionStore>>,
    pub session_id: Option<String>,
}
```

`AgentConfigBuilder` 新增（build 前设置）：
```rust
pub fn session_store(mut self, store: Arc<dyn crate::session::SessionStore>, session_id: impl Into<String>) -> Self {
    self.session_store = Some(store);
    self.session_id = Some(session_id.into());
    self
}
```

`AgentConfig` 新增（build 后链式，与 `with_hook` 对称，供 resume 重注册）：
```rust
pub fn with_session_store(mut self, store: Arc<dyn crate::session::SessionStore>, session_id: impl Into<String>) -> Self {
    self.session_store = Some(store);
    self.session_id = Some(session_id.into());
    self
}
```

### 步骤 4：`run/mod.rs` — start / resume 自动注册 + resume API

**自动注册**：抽取 helper `maybe_register_persistence(config: &mut AgentConfig)`：若 `config.session_store` 非空且 `config.session_id` 非空，构造 `SessionPersistenceHook::new(store, session_id, config.clone())` 并 push 到 `config.hooks`。`start_with_bus` 和 `resume` 在构建 args 前都调用它。注意：克隆 config 作为 `original_config` 须在 push hook **之前**（避免把 persistence hook 自身计入 original_config 的 hooks，虽然 hooks 是 serde-skip 不影响 snapshot，但语义上更干净）。

**AgentRun::resume**：

```rust
pub fn resume(
    snapshot: crate::session::SessionSnapshot,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
) -> (RunHandle, EventReceiver) {
    let config = snapshot.active_config.clone();
    // ... 构建 AgentRunArgs，但 messages 从 snapshot 来，run_id 固定 ...
    Self::resume_with_bus(snapshot, model, registry, ApprovalBus::default())
}
```

`AgentRunArgs` 或 actor 内部需要区分 "start from input" vs "resume from snapshot"。可添加一个 `initial_messages: Option<Vec<Message>>` 字段到 `AgentRunArgs`：非空时跳过 input → first user message 的构建。

### 步骤 5：`src/lib.rs` — re-export

```rust
pub mod session;
pub use session::{SessionSnapshot, SessionStore, SessionError, InMemorySessionStore, SessionPersistenceHook};
```

### 步骤 6：单元测试

1. `session_snapshot_round_trip`：序列化后反序列化，字段值相同
2. `in_memory_store_save_and_load`：save 后 load 得到等价 snapshot
3. `in_memory_store_schema_mismatch`：save schema_version="0.1" 的 snapshot，手动修改为 "0.0" 后 load 返回 SchemaMismatch
4. `persistence_hook_saves_on_run_end`：FakeModelAdapter 完成一轮 run，验证 InMemorySessionStore 中有 session
5. `persistence_hook_saves_on_run_error`：FakeModelAdapter 返回错误，验证 store 仍保存了 snapshot
6. `resume_continues_from_snapshot`：start → 完成若干步 → snapshot → resume，验证 run_id 一致、messages 从 snapshot 继续

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
