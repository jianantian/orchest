# 003 · as_tool() Builder — 实现计划

## 步骤

### 1. 定义 SubAgentBuilder
文件：新建 `crates/agent-runtime-core/src/tool/sub_agent_builder.rs` 或内联到 `agent_as_tool.rs`
- `SubAgentBuilder` struct，持有 `AgentConfig` clone + optional fields
- `model()` / `registry()` 必填（`build()` 时 panic if missing — 或返回 `Result`）
- `inherit_context(n)` 设置 `inherit_context_count: Option<usize>`
- `input_mapper` / `output_extractor` 有默认实现
- `build()` → 构造 `AgentAsTool` → 返回 `Arc<dyn Tool>`

### 2. 修改 AgentAsTool
文件：`crates/agent-runtime-core/src/tool/agent_as_tool.rs`
- 新增 `inherit_context_count: Option<usize>` 字段
- `call()` 中：如果 `inherit_context_count` 有值，从 `ctx.parent_messages` 取最后 N 条 prepend 到 messages

### 3. 扩展 ToolContext
文件：`crates/agent-runtime-core/src/tool/mod.rs`
- `ToolContext` 新增 `pub parent_messages: Vec<Message>`

### 4. 修改 ToolMetadata
文件：`crates/agent-runtime-core/src/tool/mod.rs`
- 新增 `pub needs_parent_context: bool`（默认 `false`）
- `AgentAsTool` 的 metadata 设为 `true`

### 5. 修改 run loop 的 ToolContext 构造
文件：`crates/agent-runtime-core/src/run/actor.rs`（和/或 `tool_exec.rs`）
- 构造 `ToolContext` 时：检查 `tool_meta.needs_parent_context`
  - `true` → `parent_messages: state.messages.clone()`
  - `false` → `parent_messages: vec![]`

### 6. 旧 API 标记 deprecated
文件：`crates/agent-runtime-core/src/run/config.rs`
- 旧 7 参数 `as_tool()` 加 `#[deprecated(since = "0.9.0", note = "use as_tool(name, desc).model(m).registry(r).build()")]`
- 内部实现改为调用 builder

### 7. 新增 AgentConfig::as_tool() 新签名
文件：`crates/agent-runtime-core/src/run/config.rs`
- 新增 `pub fn as_tool(&self, name: &str, description: &str) -> SubAgentBuilder`
- 旧方法改名为 `as_tool_legacy` 或类似，加 deprecated

### 8. 更新 examples 和 tests
- `examples/rust/agent_as_tool.rs` 迁移到 builder
- 新增测试：`inherit_context` 正确 prepend messages
- 新增测试：不调用 `inherit_context` 时 messages 为空（与旧行为一致）

### 9. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
