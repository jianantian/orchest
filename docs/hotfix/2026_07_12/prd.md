# Hotfix 2026-07-12 PRD:ADR-0002 Phase 1 + Phase 2 落地(protocol × provider 解耦)

## 背景

[ADR-0002](../../adr/0002-protocol-provider-decoupling.md)(已 Accepted,2026-07-11)确立
**protocol core + provider entry + provider profile** 三层模型:把 wire 协议方言从
provider 名字上解耦,让"完全兼容某协议的 provider = 纯配置(零代码)","有扩展的
provider = 一个小 profile","真正的新 wire 协议 = 一个新 ProtocolFactory"。

现状违反了这一原则:`orchest-provider-http` 里每个 provider 各自实现一整个 adapter,
四个 "OpenAI 兼容" adapter(openai/deepseek/volcengine/openrouter)把同一套 Chat
envelope 抄了四遍,又各自 fork reasoning 扩展;Minimax 在 Messages 侧 fork 了 role
与 thinking 方言;Elss 用 factory 内部 `elss/anthropic/` `elss/openai/` 前缀解析做
ad-hoc 路由,下一个 gateway 无法复用(ADR Problem 2)。

本 hotfix 落地 ADR 的 **Phase 1(protocol factory 抽取 + profile 抽取 + catalog 迁移)**
与 **Phase 2(model-string 语法 + Elss 溶解 + 消费者面)**——两者都是**非破坏**变更。
Phase 3(删除 legacy adapter/`ProviderFactory`)是**破坏性**,独立成 v0.12 重构迭代,排在
**v1.0 冻结前**(v0.x 仍可自由破坏公开 API),见
[`docs/iteration/v0_12/prd.md`](../../iteration/v0_12/prd.md)。

## 目标

1. 引入 `Protocol` / `ResolvedModel` / `ProtocolFactory` / `ProviderProfile` /
   `ProviderEntry` 机器,`ChatProtocolFactory` 与 `MessagesProtocolFactory` 各承载一套
   共享 envelope
2. 六个 LLM provider(openai/deepseek/volcengine/openrouter/anthropic/minimax)全部改走
   protocol factory:canonical 行为归 protocol core,残差归各自 profile
3. catalog 成为模型能力/上下文上限的唯一事实来源;硬编码 prefix 表降级为显式 fallback
4. `provider/[protocol/]model` 显式协议语法落地(vocabulary-based 解析),Elss 溶解为纯
   `ProviderEntry` + `protocol_aliases`,"同一 model 多协议"成为一等可发现能力
5. legacy `ProviderFactory` 全程用 bridge 保留(删除是 Phase 3 / v1.0,不在本批)

## 成功指标

- 六个 provider 的 model string(含 `openrouter/<vendor>/<model>` 多段形、`elss/anthropic/…`
  `elss/openai/…` 三段形、各种完整-endpoint `api_url` 配置)全部继续解析、行为不变
- protocol core 内**零** provider-name 条件分支(ADR 规则 1)
- `ProviderProfile` 的 hook 只按真实 provider 需要逐个引入,无 `modify_request(&mut body)`
  之类通用逃生舱(ADR 规则 3)
- `Protocol` enum 仍是 chat-scoped、留在 wall 之下
- node/py 经 `normalize_provider_model` 的路径在新 `protocol` 字段下向后兼容
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo fmt --check`、`bash scripts/lint-check.sh` 全过

## Issue 拆分

### Phase 1 — protocol factory + profile + catalog(非破坏)

| Issue | 标题 | 依赖 | 类型 |
|-------|------|------|------|
| 001 | Chat 协议 tracer:OpenAI 走 `ChatProtocolFactory`(诞生核心机器) | 无 | AFK |
| 002 | DeepSeek 首个 Chat profile(诞生 `ProviderProfile`) | 001 | AFK |
| 003 | Volcengine Chat profile(`option_support`) | 002 | AFK |
| 004 | OpenRouter Chat profile(`interpret_usage` + `HeaderValue::Env`) | 002 | AFK |
| 005 | Messages 协议 tracer:Anthropic 走 `MessagesProtocolFactory` | 001 | AFK |
| 006 | Minimax Messages profile(`map_role` + `path_overrides`) | 002 + 005 | AFK |
| 007 | catalog 成为能力事实的唯一来源 | 001 | AFK |

### Phase 2 — model-string 语法 + Elss 溶解 + 消费者面(非破坏)

| Issue | 标题 | 依赖 | 类型 |
|-------|------|------|------|
| 008 | model-string 语法:`provider/[protocol/]model` 解析 | 001 + 005 | AFK |
| 009 | Elss 溶解为纯 `ProviderEntry` + `protocol_aliases` | 008 | AFK |
| 010 | 显式协议选择:消费者面 + 文档 + binding 回归 | 009 | AFK |

依赖顺序:001 先行(walking skeleton);完成后 {002, 005, 007} 三条线可并行;002 完成后
{003, 004} 可并行;006 需 002+005;008 需 001+005(两个 factory 都在才能路由显式协议);
009 需 008;010 需 009。

每 issue 的详细 spec 见 `issues/NNN-<slug>/spec.md`。按 WORKFLOW:实施分支
`hotfix/2026_07_12`,一 issue 一 commit。GitHub issue 若需要,在各 slice 实施开始时补建
(本批以本地 spec 为准)。

## 范围裁定(全 hotfix 级)

- **非破坏**:本批全部落在 ADR 的 Phase 1-2 非破坏区间。每个 slice 的验收都含"现存测试
  全绿 + 现存 model string 行为不变"。删除 legacy adapter / `ProviderFactory` 是 Phase 3
  (v1.0),**不做**。
- **legacy bridge**:001 引入新路径后,未迁移的 provider 继续走旧 `ProviderFactory`;
  `create_adapter_from_config` 按 provider 是否已有 protocol-factory 路径分发。到 006
  完成时六个 provider 全部迁完,但旧 trait 仍保留(留给 Phase 3 删)。
- **Responses 不实现**:`Protocol::Responses` 仅作为可解析/可路由的标识符存在,背后**无**
  factory。其 stateful 语义可能要改动 protocol 层之上的 `ChatModel` trait,需单独设计评审
  (见 ADR "Protocol registry" 的 caution)。本批任何 slice 都不得为它建 factory。
- **Gemini / 其他新协议、per-protocol auth scheme 不捎带**:ADR Resolved Q2 / Q12 已裁定。
- **SDK 入参层暴露协议维度不做**:010 只更新 `.env.example`/catalog 提示与 binding 回归;
  Python/TS SDK 入参新增协议选择面是 post-hotfix 工作。

## 验收标准

- [ ] 001–010 各自 spec 的验收 checklist 全过
- [ ] 六个 provider 全部走 protocol factory;Elss 变为零 adapter 代码
- [ ] catalog 成为能力事实唯一来源,prefix 表仅作显式 fallback
- [ ] `provider/[protocol/]model` 显式语法可用且被文档化;别名向后兼容
- [ ] protocol core 无 provider-name 条件分支;`ProviderProfile` 无通用逃生舱 hook
- [ ] `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` /
      `cargo fmt --check` / `bash scripts/lint-check.sh` 全过
- [ ] ADR-0002 无需再改;若实施中发现 spec 与 ADR 冲突,先回 ADR 评审再动代码

## 依赖与后续

- 无外部依赖。全部改动在 `crates/orchest-provider-http`(+ 必要时 `orchest-provider-core`
  的 registry 类型),不动 wall(`orchest-provider`)对外面语义;node/py 仅回归验证。
- **后续 Phase 3(v0.12,破坏性,v1.0 冻结前)**:删除 legacy `ProviderFactory` trait 与冗余
  `*Adapter` 结构,registry 只存 `ProviderEntry`。独立成 v0.12 重构迭代,依赖本 hotfix 全部
  完成。详见 [`docs/iteration/v0_12/prd.md`](../../iteration/v0_12/prd.md)。
