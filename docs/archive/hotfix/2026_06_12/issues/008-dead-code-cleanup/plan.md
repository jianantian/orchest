# 008 · Dead Code 与 Dead Contract 清理 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Remove unused forward declarations and a dead tool update contract so the runtime surface only contains live paths.

**Architecture:** Delete unused `AgentRef` wrapper entirely. Remove `ToolContext.on_update` and migrate any remaining callers to the already-live `event_tx` path.

**Tech Stack:** Rust core runtime, ToolContext, in-process tools, skill executor context.

---

## 要读的现有代码

- `crates/agent-runtime-core/src/run/agent_ref.rs`
- `crates/agent-runtime-core/src/run/mod.rs`
- `crates/agent-runtime-core/src/tool/mod.rs`
- `crates/agent-runtime-core/src/tool/in_process.rs`
- `crates/agent-runtime-core/src/run/actor.rs`
- `crates/agent-runtime-core/src/tool/code_exec.rs`
- `crates/agent-runtime-core/src/skill/executor.rs`

## 文件改动

- Delete or empty/remove module: `crates/agent-runtime-core/src/run/agent_ref.rs`
- Modify: `crates/agent-runtime-core/src/run/mod.rs`
- Modify: `crates/agent-runtime-core/src/tool/mod.rs`
- Modify: `crates/agent-runtime-core/src/tool/in_process.rs`
- Modify: `crates/agent-runtime-core/src/run/actor.rs`
- Modify any test/helper constructing `ToolContext`

## 步骤

### 1. 删除 unused AgentRef

- [ ] Confirm no production code references the wrapper:

```bash
rg "AgentRef|AgentError" crates/agent-runtime-core/src crates/agent-runtime-core/tests
```

The only production hits should be `run/agent_ref.rs` and possibly a module declaration/comment.

- [ ] Delete `crates/agent-runtime-core/src/run/agent_ref.rs`.
- [ ] Remove `mod agent_ref;` or any related export from `crates/agent-runtime-core/src/run/mod.rs`.
- [ ] Ensure this does not touch Python `AgentError` exception classes; those are unrelated public binding exceptions.

### 2. 删除 `ToolContext.on_update`

- [ ] In `tool/mod.rs`, remove:

```rust
pub on_update: Option<mpsc::Sender<Value>>,
```

from `ToolContext`.

- [ ] Remove `on_update: None` from every `ToolContext` construction:

```bash
rg "on_update:" crates/agent-runtime-core/src
```

- [ ] In `tool/in_process.rs`, remove `on_update: ctx.on_update.clone()` from derived contexts. If an in-process callback needs updates, use `ctx.event_tx` and `RuntimeEvent::ToolCallUpdate` like `ExecutePythonTool`.
- [ ] Leave `ScriptExecutionContext.on_update` in `skill/executor.rs` only if it is an independent script-executor contract still used outside `ToolContext`. Do not remove it unless all references are proven dead.

### 3. Compile-driven cleanup

- [ ] Run:

```bash
cargo check -p agent-runtime-core
```

- [ ] Fix every compile error by removing stale field initializers or replacing `ctx.on_update` usage with `ctx.event_tx`.
- [ ] Do not add `#[allow(dead_code)]` to paper over removed fields.

### 4. 验证

```bash
rg "AgentRef|pub\\(crate\\) enum AgentError|pub on_update|ctx\\.on_update|on_update: None" crates/agent-runtime-core/src
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Expected:

- `AgentRef` / runtime `AgentError` no longer appear in `agent-runtime-core/src/run`.
- `ToolContext` has no `on_update`.
- Any remaining `on_update` hits are only for the separate `ScriptExecutionContext` contract if still live.

