# 002 · Error 类型治理 — 实施计划

## 依赖

无前置依赖。完成后通知 001 回来改 B1 的 `{violation:?}` → `{violation}`，通知 006 可以开始。

## 步骤

### Step 1: T7 — BudgetViolation thiserror 派生

**文件**：`crates/agent-runtime-core/src/budget.rs:22-28`

1. `thiserror = "2"` 已在 `agent-runtime-core/Cargo.toml` 中，无需添加
2. 将 `BudgetViolation` 改为 thiserror 派生：
   ```rust
   #[derive(Debug, Clone, thiserror::Error)]
   pub enum BudgetViolation {
       #[error("token limit exceeded")]
       MaxTokensExceeded,
       // ...
   }
   ```
3. 确认 `Serialize`/`Deserialize` derive 是否需要保留（检查下游使用）
4. 运行测试验证 Display 输出

### Step 2: A13 — ModelError 瘦身

**文件**：`crates/agent-runtime-providers/src/types.rs`（grep `pub struct ModelError`）

1. 找到 `ModelError` 定义，确认当前字段列表
2. 创建 `UpstreamErrorDetail` 结构体，合并 `upstream_code`、`upstream_message`、`upstream_body` 三个字段
3. `ModelError` 中替换为 `pub upstream: Option<Arc<UpstreamErrorDetail>>`
4. 更新所有构造 `ModelError` 的位置（grep `ModelError {`）：
   - `providers/anthropic.rs`
   - `providers/openai.rs`
   - `providers/deepseek.rs`
   - `providers/openrouter.rs`
   - `providers/lib.rs`
5. 删除全部 `#[allow(clippy::result_large_err)]`（当前 12 处：lib.rs×6, openrouter.rs×3, anthropic.rs×1, deepseek.rs×1, openai.rs×1）
6. 运行 `cargo clippy -- -D warnings` 确认 `result_large_err` 不再触发

### Step 3: A6 — ToolError 错误链

**文件**：`crates/agent-runtime-core/src/tool/mod.rs:99-104`

1. 决定方案：枚举化 or struct + source。评估改动面：
   - grep `ToolError {` 统计构造点数量
   - grep `From<.*> for ToolError` 统计已有转换
2. 实施选定方案，确保 `std::error::Error::source()` 可追溯到 `McpError` / `ModelError`
3. 更新所有 `From` impl
4. 更新所有 `ToolError { message, code }` 构造点
5. `ModelError` 入 `Box`（`From<Box<ModelError>>`）避免 `result_large_err`

### Step 4: 回到 001 修复 B1

001 的 B1 改为 `{violation}`（Display）。

## 文件影响范围

```
crates/agent-runtime-core/src/budget.rs           — T7
crates/agent-runtime-providers/src/types.rs        — A13 (ModelError)
crates/agent-runtime-core/src/tool/mod.rs          — A6 (ToolError)
crates/agent-runtime-core/src/tool/code_exec.rs    — ToolError 构造点
crates/agent-runtime-core/src/tool/mcp.rs          — ToolError 构造点 + From<McpError>
crates/agent-runtime-core/src/tool/builtin.rs      — ToolError 构造点
crates/agent-runtime-core/src/tool/in_process.rs   — ToolError 构造点
crates/agent-runtime-providers/src/anthropic.rs    — ModelError 构造点
crates/agent-runtime-providers/src/openai.rs       — ModelError 构造点
crates/agent-runtime-providers/src/deepseek.rs     — ModelError 构造点
crates/agent-runtime-providers/src/openrouter.rs   — ModelError 构造点
crates/agent-runtime-providers/src/lib.rs          — ModelError 构造点
```

## 验证

```bash
cargo clippy --workspace -- -D warnings   # 零 result_large_err
cargo test --workspace
grep -rn 'allow(clippy::result_large_err)' crates/ --include='*.rs' | grep -v target  # 应为空
```
