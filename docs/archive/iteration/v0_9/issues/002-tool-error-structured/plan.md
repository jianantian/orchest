# 002 · ToolError 结构化 — 实现计划

## 步骤

### 1. 定义新类型
文件：`crates/agent-runtime-core/src/tool/mod.rs`
- 新增 `ErrorKind` 枚举，derive `Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq`
- 新增 `RetryHint` 枚举，同上
- `ToolError` 新增字段 `kind: ErrorKind`、`retry: RetryHint`、`next_step: Option<String>`
- 保留 `code: Option<String>`
- 保留 `#[derive(thiserror::Error)]` 和 `#[error("{message}")]`

### 2. 实现便捷构造
文件：`crates/agent-runtime-core/src/tool/mod.rs`
```rust
impl ToolError {
    pub fn fatal(message: impl Into<String>) -> Self { ... }
    pub fn invalid_input(message: impl Into<String>) -> Self { ... }
    pub fn transient(message: impl Into<String>) -> Self { ... }
    pub fn with_code(mut self, code: impl Into<String>) -> Self { ... }
    pub fn with_next_step(mut self, hint: impl Into<String>) -> Self { ... }
}
```

### 3. 迁移 core crate 构造点
按文件逐一迁移，选择最匹配的构造函数：
- `tool/builtin.rs` — path traversal → `fatal`，IO error → `transient`
- `tool/code_exec.rs` — execution error → `fatal`
- `tool/mcp.rs` — MCP errors → `fatal().with_code(mcp_code)`
- `tool/agent_as_tool.rs` — child run error → `fatal`
- `tool/handoff_tool.rs` — config error → `invalid_input`
- `tool/search.rs` — search error → `transient`
- `skill/bundled_tool.rs` — path security → `fatal`，script error → `fatal`

### 4. 迁移 SDK crate 构造点
- `agent-runtime-py/src/lib.rs` — Python 调用错误大多 → `fatal`
- `agent-runtime-node/src/lib.rs` — Node 调用错误大多 → `fatal`

### 5. 更新测试
- 搜索所有构造 `ToolError { message: ..., code: ... }` 的测试
- 迁移到便捷构造函数
- 新增测试：验证 `fatal()` / `invalid_input()` / `transient()` 返回正确的 kind + retry 组合

### 6. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
