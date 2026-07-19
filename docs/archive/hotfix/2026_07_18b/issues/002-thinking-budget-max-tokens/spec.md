# 002 — thinking budget 与 max_tokens 组合合法性

## 背景

`RequestOptions` 默认 `thinking=Medium`;Messages 协议下非 adaptive 模型映射为 `budget_tokens=10240`(`crates/orchest-provider-http/src/messages.rs:235-244` 附近),而适配器层 max_tokens 默认 **4096**(`crates/orchest-provider-http/src/defaults.rs:4`,chat.rs / messages.rs 共用)。Anthropic 要求 `max_tokens > budget_tokens` → **默认配置 + 非 adaptive 模型(如 claude-haiku-4 系)直接 400**。

## 目标/范围

Messages 适配器构造请求时校验:thinking 开启且 `budget_tokens >= max_tokens` 时,把 max_tokens 抬升至 `budget_tokens + 完成预算`(完成预算默认 4096,即抬到 `budget_tokens + 4096`),并在 `ModelResponse.option_adjustments` 记录该调整(遵循 `docs/polaris/observability.md` "调整可见、不静默" 原则)。用户显式设置的 max_tokens ≤ budget_tokens 时同样抬升并记录(以不 400 为准;不在本 hotfix 引入报错路径)。

不改 `RequestOptions` 默认值(避免行为突变);adaptive 模型(`output_config.effort` 映射)与 Chat 协议(`reasoning_effort`)不受影响。

## 验收标准

- [x] 默认 options + 非 adaptive 模型:wire 上 `max_tokens > budget_tokens`,且 `option_adjustments` 有对应记录
- [x] 用户显式 `max_tokens ≤ budget_tokens`:同样抬升 + 记录,不产生 400
- [x] `budget_tokens < max_tokens` 的合法组合:不调整、无记录
- [x] adaptive 模型路径与 Chat 协议路径行为不变
- [x] 新增测试覆盖以上分支;四件套全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest-provider-http/src/messages.rs`(thinking/max_tokens 序列化)、`defaults.rs`、`crates/orchest-protocol/src/options.rs`、`crates/orchest-protocol/src/response.rs`(`option_adjustments` 的形状与现有用法)
- 改: `messages.rs` 请求构造处;调整记录写入 `option_adjustments`
- 测: 新增 wire 级断言 + adjustments 断言
