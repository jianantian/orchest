# Issue 004:OpenRouter Chat profile — `interpret_usage` + `HeaderValue::Env`

Parent: [ADR-0002](../../../../../adr/0002-protocol-provider-decoupling.md) Phase 1 · AFK · 依赖 002

## 现状

OpenRouter 是多 provider 路由网关,OpenAI 兼容但带三处独有行为:
- 自己的 `reasoning` 对象 + `reasoning_details` 回放数组(与 openai/deepseek/volcengine 都不同);
- 有时**不报 usage**——`OpenRouterAdapter` 今天检测到 usage 缺失时发一个 `OptionAdjustment`
  并记 telemetry;
- 两个路由 header(`X-OpenRouter-Title` / `HTTP-Referer`),header 名是静态的,值在 adapter
  构造时从环境变量 `OPENROUTER_APP_TITLE` / `OPENROUTER_SITE_URL` 读。
- model id 是多段形 `openrouter/<upstream-provider>/<model-name>`(如
  `openrouter/anthropic/claude-opus-4-8`),第二段**不是** protocol 而是上游 vendor 前缀。

## 方向(本 slice 建什么)

把 OpenRouter 迁到 `ChatProtocolFactory` + `OpenRouterProfile`。本 slice **诞生
`interpret_usage` hook,并落地 `HeaderValue::Env`**:

- `interpret_usage(&self, &ResolvedModel, &Value, &mut TokenUsage) -> Vec<OptionAdjustment>`
  ——把 provider 特有的 usage 报告(缺失 usage、cached/reasoning token)解释成 canonical
  `TokenUsage`。OpenRouter 用它承载 usage-missing 分支(原来的 `OptionAdjustment` + telemetry
  从 adapter 内迁到此 hook）。
- `lower_options` 覆盖成 OpenRouter 的 `reasoning` 对象;`replay_reasoning` 覆盖成
  `reasoning_details` 数组回放。
- `HeaderValue::Env(...)` 落地:`ProviderEntry.extra_headers` 存
  `("X-OpenRouter-Title", Env("OPENROUTER_APP_TITLE"))` 等,`ChatProtocolFactory` 在构造
  adapter 时解析 `Env` 值(runtime env 值不能是 `'static`,故 entry 存的是 env var **名**)。
- 多段 model id:确认 `openrouter/anthropic/claude-opus-4-8` 的第二段仍被当作 model 名的一部分
  (`anthropic` 既非 canonical 协议名、也非 openrouter 声明的 alias)。完整的 vocabulary-based
  解析规则在 008 收口;本 slice 至少不得让多段形回归失败。

## 落地与测试

- `openrouter/anthropic/claude-opus-4-8` 等多段 model string 行为与迁移前一致:请求体的
  `reasoning` 对象、`reasoning_details` 回放、路由 header、usage-missing 时的
  `OptionAdjustment` + telemetry(对比现有 `openrouter/tests.rs`)。
- 新增单测:`interpret_usage` 对"有 usage / 无 usage"两种响应返回正确的 `TokenUsage` 与
  adjustments;`HeaderValue::Env` 在 env 有值/无值时的解析。

## 验收标准

- [ ] `interpret_usage` hook 诞生;`HeaderValue::Static/Env` 落地并被 `ChatProtocolFactory`
      在构造时解析
- [ ] OpenRouter 走 `ChatProtocolFactory` + `OpenRouterProfile`,现存请求体/header/usage 断言全过
- [ ] `openrouter/<vendor>/<model>` 多段形回归通过(第二段仍属 model 名)
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不做完整解析规则收口(008)。不动 006 才需要的 `map_role` / `path_overrides`。
