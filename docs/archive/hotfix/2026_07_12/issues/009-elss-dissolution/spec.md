# Issue 009:Elss 溶解为纯 `ProviderEntry` + `protocol_aliases`

Parent: [ADR-0002](../../../../../adr/0002-protocol-provider-decoupling.md) Phase 2 · AFK · 依赖 008

## 现状

`ElssFactory` 用 factory 内部逻辑做 ad-hoc 路由:`parse_elss_model` 解析
`elss/anthropic/<model>` `elss/openai/<model>` 前缀、按 `claude-*` 猜协议,分别委托给
`AnthropicAdapter` / `OpenAiAdapter`;`resolve_api_url` 用后缀嗅探拼 endpoint。这套逻辑埋在
Elss 里,下一个 gateway 无法复用(ADR Problem 2)。008 已落地通用解析规则、别名机制、URL
规则;本 slice 是**收益兑现点**:Elss 变成零 adapter 代码。

## 方向(本 slice 建什么)

- **`protocol_aliases` per-provider**:Elss entry 声明 `("anthropic", Messages)`
  `("openai", Chat)`,让已发布的 `elss/anthropic/…` `elss/openai/…` 三段形继续可用(它们
  正是 Elss 自家文档里用的拼法)。别名**按 provider 作用域,绝不全局**——
  `openrouter/anthropic/claude-opus-4-8` 不受影响(openrouter 无别名声明)。
- **Elss = 纯 entry**:声明 name / base URL / key env / `protocols: [Messages, Chat]` /
  `protocol_aliases` / 无 `path_overrides` / 无 `profiles`。Elss 走 001 的
  `ChatProtocolFactory` 与 005 的 `MessagesProtocolFactory`。
- **删除**:`ElssFactory`、`parse_elss_model`、`resolve_api_url` 及其单测(行为已由 008 的
  通用解析/URL 规则承载,原单测迁移为通用规则的测试用例)。
- auto-detect 仍成立:`elss/claude-sonnet-5`→Messages、`elss/gpt-4.1`→Chat(经 008 的
  prefix 表 + provider `protocols` 过滤)。

## 落地与测试

- 回归全部 Elss 形态:`elss/claude-sonnet-5`(auto→Messages)、`elss/gpt-4.1`(auto→Chat)、
  `elss/anthropic/…`、`elss/openai/…`(经别名)、`elss/messages/…`、`elss/chat/…`(canonical
  显式),以及完整-endpoint `api_url` 配置(如 `…CHAT_API_URL=https://api.elss.ai/v1/messages`)
  ——行为与迁移前一致(迁移现有 `elss/mod.rs` 的测试)。
- 反例:`openrouter/anthropic/claude-opus-4-8` 第二段仍属 model 名(别名不全局化)。
- `grep` 确认 `ElssFactory`/`parse_elss_model`/`resolve_api_url` 已删,Elss 无 adapter 代码。

## 验收标准

- [ ] Elss 变为纯 `ProviderEntry` + `protocol_aliases`;`ElssFactory`/`parse_elss_model`/
      `resolve_api_url` 删除
- [ ] 全部 Elss 形态 + 完整-endpoint 配置回归通过
- [ ] `openrouter/<vendor>/<model>` 不受 Elss 别名影响
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不删 legacy `ProviderFactory` trait(Phase 3 / v1.0)。不为其他 provider 声明别名。
