# 008 · Dead Code 与 Dead Contract 清理

## 背景

两处代码已失去用途：一个是 v0.8 的 forward declaration 从未接入，一个是 dead contract 字段。

## 8a. `AgentRef` / `AgentError` 未使用

**文件**：`crates/agent-runtime-core/src/run/agent_ref.rs:8-20`

```rust
#[allow(dead_code)] // v0.8 forward declaration
pub enum AgentError { ... }

#[allow(dead_code)] // v0.8 forward declaration
pub struct AgentRef { ... }

#[allow(dead_code)] // justified: pub(crate) API used by RunHandle/supervisor, not all methods have callers yet
impl AgentRef { ... }
```

注释声称被 RunHandle/supervisor 使用，但实际两者都直接使用 `ActorRef<AgentMsg>`，从未引用 `AgentRef` wrapper。

**修复**：删除 `AgentRef` struct、`AgentError` enum 及其 `impl` 块。如果 `agent_ref.rs` 文件变空，删除文件并从 `mod.rs` 移除 `mod agent_ref;` 声明。

## 8b. `ToolContext.on_update` dead contract

**文件**：`crates/agent-runtime-core/src/run/actor.rs:860`

```rust
let ctx = ToolContext {
    // ...
    on_update: None,  // 始终 None
};
```

`on_update` 字段存在于 `ToolContext` 中但从未被设置为 `Some`。`ExecutePythonTool` 绕过它，直接使用 `ctx.event_tx` 发送 `ToolCallUpdate` 事件（`code_exec.rs:338-348`）。`on_update` 是冗余路径。

**修复**：

1. 从 `ToolContext` struct 中删除 `on_update` 字段
2. 删除 actor.rs:860 的 `on_update: None` 赋值
3. 检查是否有 Tool 实现引用 `ctx.on_update`——如果有，改为使用 `ctx.event_tx`（与 ExecutePythonTool 一致）

## 验收标准

- [ ] `agent_ref.rs` 中无 `AgentRef` / `AgentError`（或文件已删除）
- [ ] `ToolContext` 中无 `on_update` 字段
- [ ] 无 `#[allow(dead_code)]` 残留在被删除代码的周围
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 无 warning
