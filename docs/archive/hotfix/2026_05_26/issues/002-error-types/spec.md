# 002 · Error 类型治理

## 背景

Review 发现三个相关的错误类型问题：

- **A6**：`ToolError` 只有 `message` + `code`，无 `source`。`From<McpError>` 转换丢弃原始 error code。`ModelError`→`ToolError`→用户 的错误链断裂
- **A13**：`ModelError` 含 7 个字段（包括 `upstream_body: Option<Value>`），每个返回 `Result<_, ModelError>` 的方法都需要 `#[allow(clippy::result_large_err)]`。当前 15 处 suppress
- **T7**：AGENTS.md 约定 `BudgetError`（thiserror），实际只有 `BudgetViolation` 枚举（不是 Error trait）

## 目标

1. 所有 Error 类型实现 `std::error::Error::source()`，错误链完整可追踪
2. 消除全部 `#[allow(clippy::result_large_err)]`
3. `BudgetViolation` 对齐 AGENTS.md 约定

## A6 修复：Error 链

当前状态：

```rust
// tool/mod.rs
pub struct ToolError {
    pub message: String,
    pub code: Option<String>,
}

// From<McpError> 丢弃 McpError.code
impl From<McpError> for ToolError { ... }
```

修复：为 `ToolError` 增加 `source` 字段或枚举化 `ErrorKind`，使下游能通过 `std::error::Error::source()` 回溯到原始错误。

建议方案：

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{message}")]
    Execution {
        message: String,
        code: Option<String>,
    },
    #[error("MCP error: {0}")]
    Mcp(#[from] McpError),
    #[error("Script error: {0}")]
    Script(#[from] ScriptError),
    #[error("Model error: {0}")]
    Model(#[from] Box<ModelError>),
}
```

枚举化保留类型信息，`#[from]` 自动实现 `source()`。`ModelError` 入 `Box` 控制大小。

如果枚举化改动面太大，最小方案是保持 struct 但加 `source`:

```rust
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub message: String,
    pub code: Option<String>,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}
```

两种方案均可接受，以实际代码改动量决定。

## A13 修复：ModelError 瘦身

当前 `ModelError` 7 个字段，`upstream_body: Option<Value>` 是大头。

修复：将大字段入 `Arc` 或 `Box`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub message: String,
    pub code: Option<String>,
    pub provider: Option<String>,
    pub status: Option<u16>,
    pub upstream: Option<Arc<UpstreamErrorDetail>>,  // 合并三个 upstream 字段
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamErrorDetail {
    pub code: Option<String>,
    pub message: Option<String>,
    pub body: Option<Value>,
}
```

合并三个 `upstream_*` 字段为一个 `Option<Arc<UpstreamErrorDetail>>`：

- 减小 `ModelError` 栈大小，消除 clippy 警告
- `Arc` 使 `Clone` 零成本（当前 `Value` 的 `Clone` 是深拷贝）
- 语义更清晰：upstream detail 是一个整体

修复后删除全部 15 处 `#[allow(clippy::result_large_err)]`。

## T7 修复：BudgetViolation

当前：

```rust
pub enum BudgetViolation {
    TokenLimit, CostLimit, DurationLimit, ToolCallLimit,
}
```

不是 `Error`。AGENTS.md 约定用 `thiserror` 派生。

修复：

```rust
#[derive(Debug, thiserror::Error)]
pub enum BudgetViolation {
    #[error("token limit exceeded")]
    TokenLimit,
    #[error("cost limit exceeded")]
    CostLimit,
    #[error("duration limit exceeded")]
    DurationLimit,
    #[error("tool call limit exceeded")]
    ToolCallLimit,
}
```

同时确认 B1 的修复使用 `Display` trait（`{violation}`）而非 `Debug`（`{violation:?}`）。

## 验收标准

- [ ] `ToolError` 实现 `std::error::Error::source()`，`From<McpError>` 保留原始 error
- [ ] `ModelError` 栈大小不触发 `clippy::result_large_err`
- [ ] `crates/` 中无 `#[allow(clippy::result_large_err)]` 注解
- [ ] `BudgetViolation` 派生 `thiserror::Error`，有用户可读的 `Display` 实现
- [ ] `cargo clippy --workspace -- -D warnings` 全绿（不压制 `result_large_err`）
- [ ] 现有测试全部通过（Error 类型变更不破坏已有行为）
