# Issue 003:Volcengine Chat profile — `option_support`

Parent: [ADR-0002](../../../../../adr/0002-protocol-provider-decoupling.md) Phase 1 · AFK · 依赖 002

## 现状

Volcengine(Ark)是 OpenAI 兼容,但 thinking 字段形状与 OpenAI/DeepSeek 都不同,且**无法
排除 reasoning 输出**——`VolcengineAdapter` 为此有一条独立的 `compatibility_policy` 错误
路径(`include_thinking: false` 时按策略降级或报 `unsupported_reasoning_output_exclusion`)。
它的能力探测已经读 catalog(`catalog_entry()` → `find_model`),对未识别的 model id 保守
处理而非按名字猜。

## 方向(本 slice 建什么)

把 Volcengine 迁到 `ChatProtocolFactory` + `VolcengineProfile`。本 slice **诞生
`option_support` hook**:

- `option_support(&self, &ResolvedModel, &RequestOption) -> OptionSupport`
  ——声明某 canonical option 的支持度,让共享的 `CompatibilityPolicy` 处理统一地降级/报错,
  而不是把策略逻辑埋在 adapter 里。Volcengine 用它声明"不支持 reasoning 输出排除"。
  默认实现:在 `cx.catalog` 存在时从中推导。
- `lower_options` 覆盖成 Volcengine 的 thinking 字段形状。
- Volcengine 的 catalog 读取路径保留("读 catalog、未识别就保守处理"是 007 之后全体的参考
  范式,本 slice 不回退成 name-prefix 猜测)。

## 落地与测试

- `volcengine/<model>` 行为与迁移前一致:thinking 字段、`include_thinking: false` 时的
  策略分支(降级 vs `unsupported_reasoning_output_exclusion` 报错,对比现有
  `volcengine/tests.rs`)。
- 新增单测:`option_support` 对"支持/不支持 reasoning 排除"两种 model 返回正确;
  `CompatibilityPolicy` 据此统一处理。
- 未识别 model id 仍保守处理,不按名字猜。

## 验收标准

- [ ] `option_support` hook 诞生(含 catalog 推导的默认实现)
- [ ] Volcengine 走 `ChatProtocolFactory` + `VolcengineProfile`,现存策略分支断言全过
- [ ] reasoning 输出排除的降级/报错路径经 `option_support` + `CompatibilityPolicy` 统一处理,
      不再是 adapter 内私有分支
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不加 004/006 才需要的 hook。不动 catalog 内容(能力表迁移是 007;本 slice 只**读** catalog)。
