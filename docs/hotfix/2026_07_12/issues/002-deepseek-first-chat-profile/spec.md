# Issue 002:DeepSeek 首个 Chat profile — 诞生 `ProviderProfile`

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 1 · AFK · 依赖 001

## 现状

001 之后 `ChatProtocolFactory` 承载 canonical Chat,但没有承载"协议内残差"的机制。
DeepSeek 是 OpenAI 兼容,但 fork 了 reasoning 扩展:请求侧用 `thinking` 参数,历史回放侧
把上一轮 assistant 的 reasoning 以 `reasoning_content` 重新注入消息。今天这些逻辑埋在
`DeepSeekAdapter` 整份拷贝里。

## 方向(本 slice 建什么)

DeepSeek 是最干净的 profile 案例,用它来**诞生 `ProviderProfile` trait 与 factory 的 hook
分发**——按 ADR 规则 3,hook 由真实 provider 逐个逼出,不预先设计。

- 诞生 `ProviderProfile` trait,本 slice 只加两个 hook:
  - `lower_options(&self, &ResolvedModel, &RequestOptions, &mut Value) -> Vec<OptionAdjustment>`
    ——把 canonical option lowering 覆盖成 DeepSeek 的 `thinking` 参数方言。
  - `replay_reasoning(&self, &ResolvedModel, &mut Value, &[ContentBlock])`
    ——回放时注入 `reasoning_content`。
  - 其余 hook(`map_role` / `interpret_usage` / `option_support` / `normalize_error`)
    **本 slice 不加**,留给真正需要它们的 slice(003/004/006)诞生。
- `ChatProtocolFactory` 学会:从 `ResolvedModel.provider` 查该 `(provider, protocol)` 的
  profile(可能没有),有则在 envelope 的对应点位调用 hook,无则用 protocol-canonical 默认。
  每个 hook 都有默认实现 = canonical 行为,profile 只覆盖它真正偏离的。
- DeepSeek 从 legacy 迁到 `ChatProtocolFactory` + `DeepSeekProfile`;其 `ProviderEntry` 的
  `profiles` 字段挂上该 profile。
- profile 分发的这套"查表 → 调 hook"结构建一次,005/006 的 Messages 侧复用同一 trait。

## 落地与测试

- `deepseek/deepseek-v4-flash` 等 model string 行为与迁移前一致:请求体的 `thinking` 字段、
  `reasoning_content` 回放、`include_thinking: false` 时的 `OptionAdjustment`(对比现有
  `deepseek/tests.rs`)。
- 新增单测:profile 缺省 hook = canonical(无 profile 的 provider 走默认路径,行为不变);
  DeepSeekProfile 的 `lower_options` / `replay_reasoning` 输出正确。
- **规则 3 守门**:任何无法用具名 hook 表达的残差,必须回 spec/ADR 讨论,不得加通用
  `modify_request` 之类逃生舱。

## 验收标准

- [ ] `ProviderProfile` trait 诞生,含 `lower_options` + `replay_reasoning` 两个具名 hook
      (均有 canonical 默认实现)
- [ ] `ChatProtocolFactory` 具备 profile 查表 + hook 分发;无 profile 时行为 = canonical
- [ ] DeepSeek 走 `ChatProtocolFactory` + `DeepSeekProfile`,现存请求体/回放断言全过
- [ ] 未引入通用逃生舱 hook;未提前引入 003/004/006 才需要的 hook
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不加 `map_role` / `interpret_usage` / `option_support` / `normalize_error`(各自 slice 诞生)。
- 不动 volcengine/openrouter/anthropic/minimax。
