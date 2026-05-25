# 003 · BudgetGuard 定价解耦

## 背景

`budget.rs` 硬编码了 Anthropic 的 input/output token 价格：

```rust
// budget.rs:31-32（当前）
const INPUT_COST_PER_MILLION: f64 = 3.0;
const OUTPUT_COST_PER_MILLION: f64 = 15.0;
```

这导致：
1. 使用 OpenAI / DeepSeek / OpenRouter adapter 时，成本计算完全错误（按 Anthropic Sonnet 定价）
2. 即使是 Anthropic，也没有区分 Opus / Sonnet / Haiku 的价格差异
3. cache_read / cache_write tokens 的折扣价格被忽略

修复思路：**`BudgetGuard` 不知道任何 provider 定价。成本由 adapter 在 `TokenUsage.cost_usd` 字段直接上报。**

## 变更范围

### 1. `TokenUsage` 新增 `cost_usd` 字段

文件：`crates/agent-runtime-providers/src/types.rs`

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub details: Option<serde_json::Value>,
    pub cost_usd: Option<f64>,   // ← NEW: None = adapter 不知道定价
}
```

### 2. `ModelPricing` 类型 + `ModelCapabilities.pricing` 字段

文件：`crates/agent-runtime-providers/src/types.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cache_read_per_million_usd: Option<f64>,
    pub cache_write_per_million_usd: Option<f64>,
}

impl ModelPricing {
    pub fn calculate(&self, usage: &TokenUsage) -> f64 {
        let base = usage.input_tokens as f64 * self.input_per_million_usd / 1_000_000.0
            + usage.output_tokens as f64 * self.output_per_million_usd / 1_000_000.0;
        let cache_read = usage.cache_read_tokens.unwrap_or(0) as f64
            * self.cache_read_per_million_usd.unwrap_or(0.0) / 1_000_000.0;
        let cache_write = usage.cache_write_tokens.unwrap_or(0) as f64
            * self.cache_write_per_million_usd.unwrap_or(0.0) / 1_000_000.0;
        base + cache_read + cache_write
    }
}

// 在 ModelCapabilities 中新增字段
pub struct ModelCapabilities {
    // ... 现有字段 ...
    pub pricing: Option<ModelPricing>,   // ← NEW
}
```

### 3. Anthropic Adapter 声明定价并填充 `cost_usd`

文件：`crates/agent-runtime-providers/src/anthropic.rs`

```rust
impl AnthropicAdapter {
    fn pricing(&self) -> ModelPricing {
        match self.model.as_str() {
            m if m.contains("claude-opus-4") => ModelPricing {
                input_per_million_usd: 15.0,
                output_per_million_usd: 75.0,
                cache_read_per_million_usd: Some(1.5),
                cache_write_per_million_usd: Some(18.75),
            },
            m if m.contains("claude-sonnet-4") => ModelPricing {
                input_per_million_usd: 3.0,
                output_per_million_usd: 15.0,
                cache_read_per_million_usd: Some(0.3),
                cache_write_per_million_usd: Some(3.75),
            },
            m if m.contains("claude-haiku-4") => ModelPricing {
                input_per_million_usd: 0.8,
                output_per_million_usd: 4.0,
                cache_read_per_million_usd: Some(0.08),
                cache_write_per_million_usd: Some(1.0),
            },
            _ => ModelPricing {
                input_per_million_usd: 3.0,
                output_per_million_usd: 15.0,
                cache_read_per_million_usd: None,
                cache_write_per_million_usd: None,
            },
        }
    }
}
```

在 `complete()` 构建 `ModelResponse` 时，adapter 填充 `cost_usd`：

```rust
let pricing = self.pricing();
let cost_usd = Some(pricing.calculate(&usage));

Ok(ModelResponse {
    usage: TokenUsage { cost_usd, ..usage },
    // ...
})
```

OpenAI / DeepSeek / OpenRouter adapter 同理，各自声明自己的 `pricing()` 方法。

### 4. `BudgetGuard::record_model_call` 简化

文件：`crates/agent-runtime-core/src/budget.rs`

```rust
pub fn record_model_call(&mut self, usage: &TokenUsage) {
    self.usage.tokens_used += usage.input_tokens + usage.output_tokens;
    // 直接用 adapter 上报的成本；None 表示 adapter 不支持定价，跳过成本追踪
    if let Some(cost) = usage.cost_usd {
        self.usage.cost_usd += cost;
    }
}
```

**完全移除** `budget.rs` 中的：
```rust
const INPUT_COST_PER_MILLION: f64 = 3.0;   // ← 删除
const OUTPUT_COST_PER_MILLION: f64 = 15.0; // ← 删除
```

## 验收标准

### 类型定义

- [ ] `TokenUsage` 包含 `cost_usd: Option<f64>` 字段，`#[serde(default, skip_serializing_if = "Option::is_none")]`
- [ ] `ModelPricing` 类型存在，包含 `calculate(&self, usage: &TokenUsage) -> f64` 方法
- [ ] `ModelCapabilities` 包含 `pricing: Option<ModelPricing>` 字段

### 定价常量移除

- [ ] `budget.rs` 中不含 `INPUT_COST_PER_MILLION` 或 `OUTPUT_COST_PER_MILLION`（grep 验证）
- [ ] `BudgetGuard::record_model_call` 不做任何本地乘法计算，只读 `usage.cost_usd`

### Adapter 实现

- [ ] `AnthropicAdapter::capabilities()` 返回的 `ModelCapabilities.pricing` 为 `Some(...)`
- [ ] AnthropicAdapter 在每次 `complete()` 响应中填充 `TokenUsage.cost_usd`
- [ ] 对于 `claude-opus-4` 系列：input = $15/M，output = $75/M
- [ ] 对于 `claude-sonnet-4` 系列：input = $3/M，output = $15/M
- [ ] 对于 `claude-haiku-4` 系列：input = $0.8/M，output = $4/M
- [ ] `OpenAIAdapter::capabilities()` 返回 `ModelCapabilities { pricing: None, .. }`，`TokenUsage.cost_usd` 置 `None`（OpenAI 定价复杂，暂不填充，留给 follow-up）
- [ ] `DeepSeekAdapter::capabilities()` 同上，`pricing: None`，`cost_usd: None`
- [ ] `OpenRouterAdapter::capabilities()` 同上，`pricing: None`，`cost_usd: None`
- [ ] `grep -rn "INPUT_COST_PER_MILLION\|OUTPUT_COST_PER_MILLION" crates/` 无输出（旧常量已全部删除）

### 单元测试（内嵌在 `budget.rs`）

- [ ] 测试：`record_model_call` 在 `cost_usd = None` 时不修改 `usage.cost_usd`
  ```rust
  #[test]
  fn budget_skips_cost_when_adapter_reports_none() {
      let mut guard = BudgetGuard::new(BudgetConfig { max_cost_usd: Some(1.0), ..Default::default() });
      guard.record_model_call(&TokenUsage { input_tokens: 1000, output_tokens: 500, cost_usd: None, ..Default::default() });
      assert_eq!(guard.usage().cost_usd, 0.0);
  }
  ```
- [ ] 测试：`record_model_call` 在 `cost_usd = Some(0.01)` 时正确累加
  ```rust
  #[test]
  fn budget_accumulates_reported_cost() {
      let mut guard = BudgetGuard::new(BudgetConfig::default());
      guard.record_model_call(&TokenUsage { cost_usd: Some(0.01), ..Default::default() });
      guard.record_model_call(&TokenUsage { cost_usd: Some(0.02), ..Default::default() });
      assert!((guard.usage().cost_usd - 0.03).abs() < 1e-10);
  }
  ```
- [ ] 测试：`ModelPricing::calculate` 对 Sonnet 定价计算正确
  ```rust
  #[test]
  fn model_pricing_calculate_sonnet() {
      let pricing = ModelPricing {
          input_per_million_usd: 3.0,
          output_per_million_usd: 15.0,
          cache_read_per_million_usd: None,
          cache_write_per_million_usd: None,
      };
      let usage = TokenUsage { input_tokens: 1_000_000, output_tokens: 1_000_000, ..Default::default() };
      let cost = pricing.calculate(&usage);
      assert!((cost - 18.0).abs() < 1e-10);
  }
  ```

### 正确性

- [ ] `cargo test --workspace` 全部通过
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
