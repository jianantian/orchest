# 004 · 事件增强（ToolCallStarted + ToolCallFailed）

## 背景

`ToolCallStarted` 只携带 `tool: String, source: ToolSource, input: Value`，消费者（Watcher、审计）需要回查 registry 才知道 tool 的审批级别。`ToolCallFailed` 携带 `error: String`，丢失了结构化错误信息。

依赖 001（Approval 枚举改变了 ToolMetadata 形状）和 002（ToolError 结构化）。

## 契约

### 输入
- `ToolCallStarted { tool, source, input }`
- `ToolCallFailed { tool, error: String }`

### 输出
- `ToolCallStarted { tool, metadata: ToolMetadata, input }` — `source` 已在 `ToolMetadata` 中，不再单独列出
- `ToolCallFailed { tool, error: ToolError }` — 携带完整结构化错误

## 影响范围

- `crates/agent-runtime-core/src/events.rs` — `RuntimeEvent` 枚举定义
- `crates/agent-runtime-core/src/run/actor.rs` — emit `ToolCallStarted` / `ToolCallFailed` 的位置
- `crates/agent-runtime-core/src/run/tool_exec.rs` — emit `ToolCallFailed` 的位置
- 所有 `match RuntimeEvent::ToolCallStarted { .. }` 的位置（tests、examples、watcher）
- 所有 `match RuntimeEvent::ToolCallFailed { .. }` 的位置

## 验收标准

- [ ] `ToolCallStarted` 携带 `metadata: ToolMetadata`，移除独立的 `source` 字段
- [ ] `ToolCallFailed` 携带 `error: ToolError`（替代 `error: String`）
- [ ] 所有 emit 点传入正确的 metadata / ToolError
- [ ] 所有 pattern match 点更新
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
