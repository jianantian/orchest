# 004 · 事件增强 — 实现计划

## 步骤

### 1. 修改 RuntimeEvent 枚举
文件：`crates/agent-runtime-core/src/events.rs`
- `ToolCallStarted`：`source: ToolSource` → `metadata: ToolMetadata`
- `ToolCallFailed`：`error: String` → `error: ToolError`

### 2. 更新 ToolCallStarted emit 点
文件：`crates/agent-runtime-core/src/run/actor.rs`
- 搜索 `RuntimeEvent::ToolCallStarted`，传入完整 `ToolMetadata`（从 registry lookup 获取）

### 3. 更新 ToolCallFailed emit 点
文件：`crates/agent-runtime-core/src/run/actor.rs` + `run/tool_exec.rs`
- 搜索所有 `RuntimeEvent::ToolCallFailed { tool, error: ... }`
- 已有 `ToolError` 实例的场景：直接传入
- 只有 `String` 的场景（如 guardrail abort reason）：用 `ToolError::fatal(reason)` 包装

### 4. 更新 pattern match 点
全局搜索 `ToolCallStarted {` 和 `ToolCallFailed {`，更新解构：
- `ToolCallStarted { tool, source, input }` → `ToolCallStarted { tool, metadata, input }`（需要 `source` 时用 `metadata.source`）
- `ToolCallFailed { tool, error }` → `error` 现在是 `ToolError`，按需调 `.message` 或 `.kind`

### 5. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
