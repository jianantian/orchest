# Issue 005:Messages 协议 tracer — Anthropic 走 `MessagesProtocolFactory`

Parent: [ADR-0002](../../../../../adr/0002-protocol-provider-decoupling.md) Phase 1 · AFK · 依赖 001

## 现状

001 建了 Chat 侧的 protocol core;Messages 侧还没有。`AnthropicAdapter` 是 Anthropic
Messages API(`/v1/messages`)的 canonical 实现:content block 编码、thinking(budget_tokens
+ signature block)、cache control、SSE 解码。它今天走 legacy `ProviderFactory`。

## 方向(本 slice 建什么)

Messages 侧的 tracer,与 001 对称:抽出 `MessagesProtocolFactory`,让 `anthropic/*` 走它。
Anthropic 是 canonical Messages,**无 profile**。

- `MessagesProtocolFactory` 实现 `ProtocolFactory`(001 诞生的 trait),把 `AnthropicAdapter`
  的 canonical Messages 逻辑抽出来:content block 编码、thinking budget、cache control、SSE。
- 复用 001 的核心机器(`Protocol` / `ResolvedModel` / `ProviderConfig` + `ResolvedModel`
  签名 / `ProviderEntry`)。**本 slice 不做 profile 分发**——Anthropic 无残差;Messages 侧的
  profile 分发由 006(Minimax)复用 002 诞生的 `ProviderProfile` trait 接入。因此本 slice
  只依赖 001,不依赖 002,可与 002 并行。
- URL:`default_base_url` + 追加 `/v1/messages`(幂等 append,覆盖现有行为;完整规则 008 收口)。
- Anthropic 从 legacy 迁到 `MessagesProtocolFactory`。

## 落地与测试

- `anthropic/claude-sonnet-5` / `claude-fable-5` / `claude-opus-4-8` 等行为与迁移前一致:
  content block 序列化、thinking budget/signature、cache control、usage(含
  `cache_read_input_tokens`)(对比现有 `anthropic/tests.rs`,该文件测试量最大,是主要回归网)。
- 新增单测:`MessagesProtocolFactory` 追加 canonical path、幂等;`ResolvedModel` 解析正确。
- Chat 侧(001/002 已迁的 openai/deepseek)与其余 legacy provider 全绿。

## 验收标准

- [ ] `MessagesProtocolFactory` 落地,Anthropic 走新路径,`anthropic/tests.rs` 全套回归通过
- [ ] 复用 001 的核心机器;本 slice 不引入 Messages 侧 profile 分发(留 006)
- [ ] 无 profile 时行为 = canonical Messages
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不做 Messages profile 分发、不动 Minimax(006)。不为 `Protocol::Responses` 建 factory。
