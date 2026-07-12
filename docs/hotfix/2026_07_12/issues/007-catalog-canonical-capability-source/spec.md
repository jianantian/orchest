# Issue 007:catalog 成为能力事实的唯一来源

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 1("Capability metadata")· AFK · 依赖 001

## 现状

能力事实的来源不统一:Volcengine 读 catalog(`catalog_entry()` → `find_model`,对未识别
model 保守处理),而 OpenAI 把能力表硬编码在 adapter 里——`supports_reasoning_model`
(按 `o1`/`o3`/`o4`/`gpt-5` 前缀猜)和 `openai_context_window`(按 model 名返回上下文上限)。
001 已把 `ResolvedModel.catalog` 字段接上现有查找,但 OpenAI 仍走自己的硬编码表。

## 方向(本 slice 建什么)

落地 ADR "Capability metadata" 原则:**catalog 是模型能力与上下文上限的唯一事实来源**;
protocol core 与 profile 从 `ResolvedModel.catalog` 读,不各自维护 model 知识。

- 把 OpenAI 的 `supports_reasoning_model` / `openai_context_window` 承载的事实(reasoning
  支持、上下文窗口)迁进 catalog 条目。
- 这两个 prefix 函数**降级为显式 fallback**:仅在 catalog 缺条目时(dynamic-gateway 模型、
  未收录的 preview)使用,并在代码/注释里显式标注是 fallback,不得再作为主路径。参考范式
  是 Volcengine 现有的"读 catalog、未识别就保守处理,而非按名字猜"。
- 全体 protocol core / profile 统一从 `ResolvedModel.catalog` 取能力事实(如
  `option_support` 的默认实现、reasoning 是否可用)。

本 slice 只依赖 001(需要 `ResolvedModel.catalog` 字段),与 002–006 正交,可穿插。若在
002–006 之前落地,则那些 slice 的 profile 直接读 catalog;若之后,则各 slice 先临时读、
本 slice 收口统一。

## 落地与测试

- OpenAI 的 reasoning 判定与上下文窗口从 catalog 得出,已收录模型行为与迁移前一致;未收录
  模型走标注过的 fallback。
- 新增/调整单测:catalog 命中时能力事实取自 catalog;catalog 缺失时走 fallback 并可观测到
  是 fallback 路径。
- `grep` 确认没有"以 name-prefix 表为主来源"的残留(仅剩显式标注的 fallback)。

## 验收标准

- [ ] OpenAI 能力事实(reasoning 支持、上下文窗口)迁入 catalog
- [ ] `supports_reasoning_model` / `openai_context_window` 仅作显式标注的 catalog-缺失 fallback
- [ ] protocol core / profile 统一从 `ResolvedModel.catalog` 读能力事实
- [ ] 已收录模型行为不变;未收录模型走 fallback
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不改 pricing 键法(仍 `provider/model`,ADR Resolved Q3);本 slice 只统一"能力/上限"来源。
