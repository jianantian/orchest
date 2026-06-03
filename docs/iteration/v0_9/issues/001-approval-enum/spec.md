# 001 · Approval 枚举

## 背景

`ToolMetadata.requires_approval: bool` 只能表达"要/不要"。三态 `Approval` 枚举让用户在注册 tool 时被迫分类风险级别，同时让 Hook / Watcher 代码可以基于枚举做模式匹配，不依赖 tool name 字符串。

## 契约

### 输入
- 现有 `requires_approval: bool` 在 `ToolMetadata` 中
- `ApprovalMode::SideEffectOnly` 变体
- `RuntimeConfig::should_approve()` 基于 bool 分派

### 输出
- `Approval` 枚举（`Never / WhenRisky / Always`），默认 `WhenRisky`
- `ToolMetadata.approval: Approval` 替代 `requires_approval`
- `should_approve()` 基于枚举分派
- `SideEffectOnly` 标记 `#[deprecated]`
- SDK（Python / Node）解析保留旧值但发 warning

## 影响范围

- `crates/agent-runtime-core/src/tool/mod.rs` — `ToolMetadata` 结构体
- `crates/agent-runtime-core/src/run/config.rs` — `ApprovalMode`、`should_approve()`
- `crates/agent-runtime-core/src/tool/builtin.rs` — `ReadFileTool`、`WriteFileTool`
- `crates/agent-runtime-core/src/tool/agent_as_tool.rs` — metadata 构造
- `crates/agent-runtime-core/src/tool/handoff_tool.rs` — metadata 构造
- `crates/agent-runtime-core/src/tool/search.rs` — metadata 构造
- `crates/agent-runtime-core/src/tool/registry.rs` — MCP tool metadata 构造
- `crates/agent-runtime-core/src/tool/mcp.rs` — MCP tool metadata 构造
- `crates/agent-runtime-core/src/tool/code_exec.rs` — metadata 构造
- `crates/agent-runtime-core/src/skill/types.rs` — skill metadata
- `crates/agent-runtime-core/src/skill/bundled_tool.rs` — bundled tool metadata
- `crates/agent-runtime-core/src/hook/loop_detection.rs` — synthetic tool metadata
- `crates/agent-runtime-py/src/lib.rs` — `requires_approval` 参数 → `approval` 参数
- `crates/agent-runtime-node/src/lib.rs` — 同上

## 验收标准

- [ ] `Approval` 枚举定义，`#[default]` 为 `WhenRisky`
- [ ] `ToolMetadata.approval: Approval`，`requires_approval` 字段移除
- [ ] `should_approve()` 新逻辑：`Never → false`、`WhenRisky → meta.side_effect`、`Always → true`
- [ ] `SideEffectOnly` 变体标记 `#[deprecated]`
- [ ] 所有 metadata 构造点迁移完成（迁移映射：`true → Always`、`false → Never`）
- [ ] SDK 解析保留旧字符串值，发出 deprecation warning
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
