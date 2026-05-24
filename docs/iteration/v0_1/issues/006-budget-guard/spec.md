# 006 · Budget Guard

## 背景

防止 agent run 因失控的 token 消耗、tool call 次数或运行时长无限消耗资源。每次模型调用和 tool 调用完成后检查。

## 目标

实现 `BudgetGuard`，在 run loop 的关键节点检查预算，超限时终止 run。

## 验收标准

- [ ] `BudgetGuard::new(config: BudgetConfig) -> BudgetGuard`
- [ ] `BudgetGuard::record_model_call(usage: &TokenUsage)` 累计 token 使用和估算费用
- [ ] `BudgetGuard::record_tool_call()` 累计 tool call 次数
- [ ] `BudgetGuard::check() -> Option<BudgetViolation>` 返回第一个超限的维度，无超限返回 `None`
- [ ] `BudgetViolation` 枚举：`MaxTokensExceeded`、`MaxToolCallsExceeded`、`MaxDurationExceeded`、`MaxCostExceeded`
- [ ] `BudgetConfig` 四个字段全部可选（`None` 表示不限制）
- [ ] `max_duration` 从 run 开始计时，**包含**异步 job 等待时间
- [ ] 超限时发出 `BudgetWarning` 事件，然后发出 `RunFailed { error: "budget_exceeded" }`
- [ ] `BudgetUsage` 可序列化，作为 `RunState.budget_used` 的类型

## Token 费用估算

v0.1 使用硬编码的 Anthropic 价格表（per-million-token 粒度），不做实时价格查询。费用估算仅用于 `max_cost_usd` 检查，不保证精确。
