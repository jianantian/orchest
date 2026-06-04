# 002 · ToolError 结构化

## 背景

`ToolError { message, code }` 无法让消费者（Hook、Watcher、RetryPolicy）程序化区分错误类型。新增 `ErrorKind` 和 `RetryHint` 让错误自描述，便于 LlmWatcher 判断是否干预。

## 契约

### 输入
- 现有 `ToolError { message: String, code: Option<String> }`
- ~55 处 `ToolError { ... }` 构造（含测试）
- `ToolCallFailed { tool: String, error: String }` 事件

### 输出
- `ToolError { message, kind, retry, code, next_step }`
- `ErrorKind` 枚举（`InvalidInput / NotSupported / Transient / Fatal`），默认 `Fatal`
- `RetryHint` 枚举（`Safe / Caution / Unsafe`），默认 `Unsafe`
- 便捷构造：`fatal()` / `invalid_input()` / `transient()` + `with_code()` / `with_next_step()`
- `ToolCallFailed` 事件改为携带 `ToolError`（留到 004 issue 实施）

## 影响范围

- `crates/agent-runtime-core/src/tool/mod.rs` — `ToolError` 结构体 + 新类型
- `crates/agent-runtime-core/src/tool/builtin.rs` — error 构造
- `crates/agent-runtime-core/src/tool/code_exec.rs` — error 构造
- `crates/agent-runtime-core/src/tool/mcp.rs` — error 构造（保留 MCP error code via `with_code`）
- `crates/agent-runtime-core/src/tool/agent_as_tool.rs` — error 构造
- `crates/agent-runtime-core/src/tool/handoff_tool.rs` — error 构造
- `crates/agent-runtime-core/src/tool/search.rs` — error 构造
- `crates/agent-runtime-core/src/skill/bundled_tool.rs` — error 构造
- `crates/agent-runtime-py/src/lib.rs` — ToolError 构造
- `crates/agent-runtime-node/src/lib.rs` — ToolError 构造

## 设计决策

- `code` 字段保留：`ErrorKind` 是语义分类，`code` 是 provider-specific 标识（MCP 协议等），两者正交
- `Display` format：暂保持 `#[error("{message}")]` 不变（向后兼容日志），如需改进在后续迭代处理
- `RetryHint` 与 `RetryPolicy` 无直接关系：`RetryPolicy` 处理 model API 错误，`RetryHint` 标注 tool 错误的重试安全性

## 验收标准

- [ ] `ErrorKind` 枚举，`#[default]` 为 `Fatal`
- [ ] `RetryHint` 枚举，`#[default]` 为 `Unsafe`
- [ ] `ToolError` 新增 `kind`、`retry`、`next_step` 字段，保留 `code`
- [ ] 便捷构造函数 `fatal()` / `invalid_input()` / `transient()` 可用
- [ ] 链式方法 `with_code()` / `with_next_step()` 可用
- [ ] 所有现有 ToolError 构造点迁移完成
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
