# Issue 001:Chat 协议 tracer — OpenAI 走 `ChatProtocolFactory`

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 1 · AFK · 无依赖

## 现状

`orchest-provider-http` 里六个 provider 各自实现整个 adapter,经 `ProviderRegistry` 的
`ProviderFactory::create_adapter(model, max_tokens, api_key, api_url)` 构造。OpenAI 的
`OpenAiAdapter` 是 Chat Completions 方言的 canonical 实现(messages 数组、tool-call 组装、
SSE 解码、`stream_options.include_usage`)。ADR 三层模型的机器一件都还没有。

## 方向(本 slice 建什么)

这是 walking skeleton:驱动**一个真实 model string**(`openai/gpt-4.1`)走完整条新路径
——解析 → 解析出 `ResolvedModel` → `ChatProtocolFactory` → 构造 adapter → 测试全绿——
其余 provider 全部保持走 legacy 路径不变。机器在"让这一条路跑通"的过程中诞生,不做成
独立的"纯脚手架" ticket。

需要诞生(仅够 OpenAI 这一条路跑通的量):

- `Protocol` enum(`Messages` / `Chat` / `Responses`),**chat-scoped、留在
  `orchest-provider-http` 之内**,不出现在 wall 级或 consumer 级类型里。`Responses` 此刻
  仅是标识符,无 factory。
- `ResolvedModel<'a>`:`{ provider: &ProviderEntry, protocol, model, catalog:
  Option<&LlmModelEntry> }`。由 registry 在解析 model string 时一次性解析出;`catalog`
  字段接现有的 catalog 查找(`find_model`),此刻只是把已有查找包进来,不改 catalog 本身
  (OpenAI 能力表的迁移是 007)。
- `ProtocolFactory` trait:`create_adapter(&self, &ProviderConfig, &ResolvedModel) ->
  Result<Box<dyn ChatModel>, ProtocolError>`。**注意签名用 `ProviderConfig`(ADR-0001
  的 wall 已有类型)+ `ResolvedModel`,不是已废弃的 positional 四参签名。**
- `ChatProtocolFactory`:把 `OpenAiAdapter` 的 canonical Chat 逻辑抽出来。此刻**不引入**
  `ProviderProfile`——OpenAI 就是 canonical Chat,无残差;profile 由 002(DeepSeek)按真实
  需要诞生。
- `ProviderEntry` 的修订形态(新字段 `protocol_aliases` / `path_overrides` /
  `extra_headers: HeaderValue` / `profiles` 都先声明、此刻留空),够表达 OpenAI 一个 entry
  即可。`default_base_url` 语义 = base URL(非完整 endpoint);`ChatProtocolFactory` 追加
  `/v1/chat/completions`(URL 解析的完整规则在 008 收口,本 slice 先实现 append + 幂等,
  覆盖 OpenAI 现有 `normalize_chat_url` 行为)。

legacy bridge:`create_adapter_from_config` 按 provider 是否已有 protocol-factory 路径分发
——OpenAI 走新路径,其余走旧 `ProviderFactory`。二者共存到 006,旧 trait 留到 Phase 3 删。

## 落地与测试

- OpenAI 经新路径构造 adapter,`openai/gpt-4.1`、`openai/gpt-5.4`、带自定义 `api_url` 的
  形态行为与迁移前逐字节一致(对比现有 `openai/tests.rs` 的请求体断言)。
- 新增单测覆盖:model string 解析出正确的 `ResolvedModel`;`ChatProtocolFactory` 追加
  canonical path、幂等(传入完整 endpoint 不重复追加)。
- 其余五个 provider 走 legacy,现存测试不动、全绿。

## 验收标准

- [ ] `Protocol` / `ResolvedModel` / `ProtocolFactory` / `ChatProtocolFactory` /
      修订版 `ProviderEntry` 落地,OpenAI 走新路径
- [ ] `Protocol` 未泄漏到 wall/consumer 级类型(`grep` 确认)
- [ ] `ProtocolFactory` 签名用 `ProviderConfig` + `ResolvedModel`,非 positional 四参
- [ ] 未引入 `ProviderProfile`(留给 002)
- [ ] OpenAI 现存请求体断言全过;其余 provider 走 legacy、测试全绿
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不引入 `ProviderProfile`、不迁移 catalog 能力表、不动其余五个 provider。
- 不为 `Protocol::Responses` 建 factory。
