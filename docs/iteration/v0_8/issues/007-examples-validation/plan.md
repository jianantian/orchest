# 007 · Examples + Final Validation — 实施计划

## 前置条件

- 001-006 全部合入 main
- `cargo test --workspace` 全绿

---

## 步骤

### 步骤 1：新建 Rust examples

每个 example 独立文件，**文件内联**一个最小 mock `ModelAdapter`（examples 看不到 `#[cfg(test)]` 的 `FakeModelAdapter`，见 spec；参考 `examples/rust/hook_logging.rs` 的 `// ── Mock model ──` 段）。

**示例：`examples/rust/guardrail_keyword_filter.rs`**

```rust
//! ToolInputGuardrail example: block tool calls containing banned keywords.
//! Run: cargo run --example guardrail_keyword_filter

use agent_runtime_core::{...};

struct KeywordBlockGuardrail { banned: Vec<String> }

#[async_trait]
impl ToolInputGuardrail for KeywordBlockGuardrail {
    // 注意：check 是只读 &ctx（不是 &mut）——guardrail 只决策，adapter 负责改 ctx
    async fn check(&self, ctx: &ToolHookContext) -> ToolInputGuardrailAction {
        let input_str = ctx.tool_input.to_string();
        for word in &self.banned {
            if input_str.contains(word.as_str()) {
                return ToolInputGuardrailAction::Reject(
                    format!("blocked: input contains banned keyword '{word}'")
                );
            }
        }
        ToolInputGuardrailAction::Allow
    }
}
```

按同等结构依次实现 6 个 example。

**`session_persist_resume.rs`** 的关键逻辑：

```rust
// 1. Start first run
let store = Arc::new(InMemorySessionStore::default());
let config = AgentConfig::builder("fake/model")
    .session_store(Arc::clone(&store), "my-session")
    .build()?;
let (handle, mut rx) = AgentRun::start(config, "Hello".into(), model.clone(), registry.clone());
// ... drain events ...
handle.wait().await;

// 2. Load snapshot
let snapshot = store.load("my-session").await?.unwrap();
assert!(!snapshot.messages.is_empty());

// 3. Resume
let (handle2, mut rx2) = AgentRun::resume(snapshot, model, registry);
// ... verify run continues, run_id same as first run ...
```

**`watcher_inject_message.rs`** 的关键逻辑：

```rust
let (handle, rx) = AgentRun::start(...);
let watcher = Arc::new(MyWatcher::new());
handle.attach_watcher(watcher, 1024).await;
// FakeModelAdapter 触发若干 ToolCall，watcher 在 ToolCallCompleted 后 Inject
```

### 步骤 2：新建 `tests/v08_integration.rs`

参考 `tests/v07_integration.rs` 的结构：

```rust
mod helpers;  // 或 use 现有 helpers

#[tokio::test]
async fn guardrail_and_approval_coexist() { ... }

#[tokio::test]
async fn session_resume_with_hooks() { ... }

// ...
```

**关键：`all_v08_features_combined`**

构建一个含以下配置的 run：
- `InMemorySessionStore` + session_id
- `KeywordBlockGuardrail`（ToolInputGuardrail）
- `ApprovalMode::SideEffectOnly`
- watcher（记录收到的事件，遇到特定条件 Inject 一条消息）
- FakeModelAdapter：第一轮返回两个 tool call（一个 side_effect=true，一个有禁用词）；第二轮收到 inject 消息后返回文本结束

验证：
- side_effect tool 触发审批（在测试中自动 approve）
- 含禁用词的 tool call 被 guardrail Reject，reason 出现在下一轮 model messages
- inject 消息出现在最终 messages 历史中
- session store 中有正确 snapshot

### 步骤 3：SQLite 集成测试

在 `tests/sqlite_integration.rs`（或 `v08_integration.rs` 底部 `#[cfg(feature="sqlite-session")]` block）：

```rust
#[cfg(feature = "sqlite-session")]
#[tokio::test]
async fn sqlite_persist_and_resume() {
    let store = Arc::new(SqliteSessionStore::open_in_memory().unwrap());
    // ... 同 in-memory 测试逻辑 ...
}
```

### 步骤 4：lint-check.sh 文件长度警告

运行 `bash scripts/lint-check.sh` 确认没有新的阻塞问题（WARN 可接受，FAIL 需修复）。若 `guardrail/mod.rs` 或 `session/mod.rs` 超长，在此步骤拆分。

### 步骤 5：最终 CI 确认

```bash
# 默认 feature
cargo fmt --check
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint-check.sh

# sqlite-session feature
cargo test --workspace --features agent-runtime-core/sqlite-session
cargo clippy --workspace --features agent-runtime-core/sqlite-session -- -D warnings
```

全绿后提交 `closes #<v0.8 final issue>` 的 commit。
