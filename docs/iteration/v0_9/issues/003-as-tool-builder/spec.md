# 003 · as_tool() Builder 模式

## 背景

`AgentConfig::as_tool()` 有 7 个参数（含 2 个闭包），代码注释已标注 `// a builder is planned for v0.8`。Builder 模式减少调用方负担，`inherit_context(n)` 为 Supervised Delegation 提供上下文继承能力。

## 契约

### 输入
- 现有 7 参数 `AgentConfig::as_tool()`
- `ToolContext` 无 `parent_messages` 字段

### 输出
- `SubAgentBuilder` — fluent builder，`build()` 返回 `Arc<dyn Tool>`
- `inherit_context(n)` — 延迟绑定，执行时从 `ToolContext.parent_messages` 取最后 N 条
- `ToolContext.parent_messages: Vec<Message>` — run loop 填充
- 旧 7 参数 `as_tool()` 标记 `#[deprecated]`

## 影响范围

- `crates/agent-runtime-core/src/run/config.rs` — `AgentConfig::as_tool()` 改为返回 builder，旧方法 deprecated
- `crates/agent-runtime-core/src/tool/agent_as_tool.rs` — `AgentAsTool` 新增 `inherit_context_count` 字段 + call 时 prepend messages
- `crates/agent-runtime-core/src/tool/mod.rs` — `ToolContext` 新增 `parent_messages`
- `crates/agent-runtime-core/src/run/actor.rs` — 构造 `ToolContext` 时填充 `parent_messages`
- `crates/agent-runtime-core/src/run/tool_exec.rs` — 同上（如果 ToolContext 在此构造）
- `examples/rust/agent_as_tool.rs` — 迁移到 builder API

## 设计决策

- `parent_messages` 性能：run loop 仅在调用 `AgentAsTool` 时才填充（通过 `ToolMetadata` 标记 `needs_parent_context: bool` 或 tool 类型检查）；其他 tool 填 empty vec
- `model` 和 `registry` 在 builder 上必填（无默认值），保持与旧 API 语义一致
- `input_mapper` / `output_extractor` 有默认实现（identity mapper），只在需要自定义时调用

## 验收标准

- [ ] `SubAgentBuilder` 类型，支持 `model()` / `registry()` / `inherit_context()` / `input_schema()` / `input_mapper()` / `output_extractor()` / `build()`
- [ ] `inherit_context(n)` 延迟绑定，`AgentAsTool::call()` 从 `ctx.parent_messages` 取最后 N 条 prepend
- [ ] `ToolContext` 新增 `parent_messages` 字段
- [ ] run loop 仅对需要的 tool 填充 `parent_messages`
- [ ] 旧 7 参数 `as_tool()` 标记 `#[deprecated]`，内部转为 builder
- [ ] `examples/rust/agent_as_tool.rs` 使用新 builder API
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
