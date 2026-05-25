# 003 实现路线

## 步骤

1. **在 `agent-runtime-providers/src/types.rs` 添加新类型**
   - 在 `TokenUsage` 末尾加 `pub cost_usd: Option<f64>` 字段，带 `#[serde(default, skip_serializing_if = "Option::is_none")]`
   - 新增 `ModelPricing` 结构体和 `calculate()` 方法（见 spec）
   - 在 `ModelCapabilities` 末尾加 `pub pricing: Option<ModelPricing>` 字段，带 `#[serde(default, skip_serializing_if = "Option::is_none")]`
   - 运行 `cargo build -p agent-runtime-providers` 确认编译通过

2. **在 `budget.rs` 移除定价常量，简化 `record_model_call`**
   - 删除 `const INPUT_COST_PER_MILLION: f64 = 3.0;` 和 `const OUTPUT_COST_PER_MILLION: f64 = 15.0;`
   - 把 `record_model_call` 中的乘法计算替换为直接读 `usage.cost_usd`（见 spec）
   - 运行 `cargo build -p agent-runtime-core` 确认编译通过

3. **在 Anthropic adapter 添加 `pricing()` 方法并填充 `cost_usd`**
   - 在 `AnthropicAdapter` 上添加 `fn pricing(&self) -> ModelPricing`，按 model 名前缀返回不同定价（见 spec 中的 match 表达式）
   - 在 `capabilities()` 返回的 `ModelCapabilities` 中设置 `pricing: Some(self.pricing())`
   - 在 `complete()` 构建 `ModelResponse` 时，计算 `cost_usd` 并填入 `usage`：
     ```rust
     let cost_usd = Some(self.pricing().calculate(&usage));
     // usage 是局部变量，在 return 前修改
     usage.cost_usd = cost_usd;
     ```
   - 运行 `cargo build -p agent-runtime-providers` 确认编译通过

4. **在其他三个 adapter 做相同改动**
   - `openai.rs`：添加 `pricing()` 方法（当前 OpenAI 定价差异大，可以先用保守的默认值，如 GPT-4o 定价 $2.5/$10 per million）
   - `deepseek.rs`：添加 `pricing()` 方法（DeepSeek-Chat: $0.14/$0.28 per million）
   - `openrouter.rs`：OpenRouter 的价格因 model 而异，暂时返回 `None`（`pricing: None`），表示不支持成本追踪，待后续完善

5. **添加测试**
   - 在 `budget.rs` 的 `#[cfg(test)]` 块中添加 spec 要求的两个测试（`budget_skips_cost_when_adapter_reports_none`、`budget_accumulates_reported_cost`）
   - 在 `types.rs` 的 `#[cfg(test)]` 块中添加 `model_pricing_calculate_sonnet` 测试

6. **验收**
   - `grep -r "INPUT_COST_PER_MILLION\|OUTPUT_COST_PER_MILLION" crates/` — 无输出
   - `cargo test --workspace` 全绿
   - `cargo clippy --workspace -- -D warnings` 全绿

## 要读的现有代码

- `crates/agent-runtime-core/src/budget.rs` — 完整文件，了解 `record_model_call` 当前实现
- `crates/agent-runtime-providers/src/anthropic.rs` — `complete()` 函数结尾处构建 `ModelResponse` 的位置，以及 `capabilities()` 的返回值
- `crates/agent-runtime-providers/src/types.rs` — `TokenUsage` 和 `ModelCapabilities` 的现有字段

## 关键决策

- **`TokenUsage.cost_usd = None` vs `Some(0.0)`**：对于不支持定价的 provider（如 OpenRouter），返回 `None` 表示"未知"，而不是 `Some(0.0)`——两者语义不同，`BudgetGuard` 对 `None` 跳过计算，对 `Some(0.0)` 会把 cost 设为 0
- **Anthropic 定价的 model 匹配**：用 `m.contains("claude-opus-4")` 等字符串包含检查，而不是精确匹配——避免因 minor version 变化导致匹配失败，fallback 用 Sonnet 定价
- **`BudgetConfig.max_cost_usd` 检查行为**：如果所有 adapter 都返回 `None`，`cost_usd` 始终是 0，`max_cost_usd` 限制形同虚设——这是已知 gap，在 spec non-goals 中说明，不在本 issue 解决
