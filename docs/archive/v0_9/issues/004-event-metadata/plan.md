# 004 · 事件增强 — 实现计划

## 步骤

### 1. 修改 RuntimeEvent 枚举
文件：`crates/agent-runtime-core/src/events.rs`
- `ToolCallStarted`：`source: ToolSource` → `metadata: ToolMetadata`
- `ToolCallFailed`：`error: String` → `error: ToolError`

### 2. 更新 ToolCallStarted emit 点
文件：`crates/agent-runtime-core/src/run/actor.rs`
- 当前只有 1 处 emit（actor.rs:840），`tool_meta` 已在作用域内
- 改为 `metadata: tool_meta.clone()`（替代 `source: tool_meta.source.clone()`）

### 3. 更新 ToolCallFailed emit 点
文件：`crates/agent-runtime-core/src/run/actor.rs` + `run/tool_exec.rs`

逐处迁移，根据错误来源选择正确的 `ErrorKind` + `RetryHint`：

**actor.rs（4 处）：**
- `actor.rs:700` — `error` 来自 tool 执行（已有 `ToolError` 上下文）→ `ToolError::fatal(error)`
- `actor.rs:824` — `"tool call budget exceeded"` → `ToolError::fatal("tool call budget exceeded")`
- `actor.rs:878` — `"tool execution timed out"` → `ToolError::transient("tool execution timed out")`
- `actor.rs:1058` — `e.message.clone()` 其中 `e: ToolError` → 直接传 `e.clone()`

**tool_exec.rs（5 处）：**
- `tool_exec.rs:50` — `JobStatus::Failed(error: String)` → `ToolError::fatal(error)`
- `tool_exec.rs:85` — `"async job timed out"` → `ToolError::transient("async job timed out")`
- `tool_exec.rs:98` — `"async job has no polling fallback"` → `ToolError::fatal("async job has no polling fallback").with_next_step("implement poll() for this async tool")`
- `tool_exec.rs:136` — `err: String` → `ToolError::fatal(err)`
- `tool_exec.rs:147` — `e.message.clone()` 其中 `e: ToolError` → 直接传 `e.clone()`

**注意**：`JobStatus::Failed(String)` 类型本身不改（它是 async tool 协议的一部分），在 emit 处包装为 `ToolError` 即可。

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
