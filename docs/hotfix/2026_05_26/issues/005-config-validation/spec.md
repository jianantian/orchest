# 005 · AgentConfig 校验

## 背景

v0.6 重构了 `AgentConfig` 为分组 builder 模式，但 `build()` 永远返回 `AgentConfig`。空 `system_prompt`、空 `model`、负 `max_cost_usd`、`max_steps: 0` 全部静默接受，推迟到运行时才发现。

## 目标

`build()` 返回 `Result<AgentConfig, ConfigError>`，在构建时拦截无效配置。

## 设计

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("model must be specified")]
    MissingModel,
    #[error("system_prompt must not be empty")]
    EmptySystemPrompt,
    #[error("max_steps must be > 0, got {0}")]
    InvalidMaxSteps(u32),
    #[error("max_cost_usd must be non-negative, got {0}")]
    InvalidMaxCost(f64),
    #[error("max_tokens must be > 0, got {0}")]
    InvalidMaxTokens(u64),
}
```

校验规则：

| 字段 | 规则 |
|------|------|
| `model` | 必须非空 |
| `system_prompt` | 必须非空（允许空白但不允许零长度） |
| `max_steps` | 必须 > 0 |
| `budget.max_cost_usd` | 若设置，必须 ≥ 0 |
| `budget.max_tokens` | 若设置，必须 > 0 |
| `budget.max_tool_calls` | 若设置，必须 > 0 |

其他字段（`allowed_tools`、`allowed_skills`、`mcp_servers` 等）允许任意值。

## 影响范围

`build()` 签名变更会影响所有构造 `AgentConfig` 的位置：

- 测试代码（最多）
- 示例代码
- SDK binding 层（`agent-runtime-py`、`agent-runtime-node`）

测试中可以用 `build().unwrap()` 或引入一个 `#[cfg(test)]` 的 `build_unchecked()` 减少噪声。

## 验收标准

- [ ] `AgentConfigBuilder::build()` 返回 `Result<AgentConfig, ConfigError>`
- [ ] 空 model、空 system_prompt、`max_steps: 0`、负 `max_cost_usd` 各有对应的 `ConfigError` 变体
- [ ] 测试覆盖每种无效配置
- [ ] 现有测试适配新签名后全部通过
- [ ] `cargo test --workspace` 全绿
