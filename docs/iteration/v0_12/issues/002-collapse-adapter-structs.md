# 002 · Collapse redundant `*Adapter` structs into protocol cores

Parent: [ADR-0002](../../../adr/0002-protocol-provider-decoupling.md) Phase 3 · [v0.12 PRD](../prd.md)

## Background

During Phase 1 the protocol factories were extracted by *wrapping* the existing adapter
logic — `ChatProtocolFactory` around `OpenAiAdapter`, `MessagesProtocolFactory` around
`AnthropicAdapter`. The OpenAI-compat vendors (`DeepSeekAdapter`, `VolcengineAdapter`,
`OpenRouterAdapter`) and `MinimaxAdapter` still exist as distinct types even though their
behavior is now fully expressed as protocol core + provider profile. These redundant structs
are the last piece of the transitional state.

## Goal

Internalize the canonical implementations into the protocol cores and delete the redundant
vendor adapters. After this issue, each provider's behavior is fully "protocol core +
`ProviderEntry` + profile" with no vendor-named adapter type.

## Acceptance Criteria

- [ ] `OpenAiAdapter` logic internalized as the `ChatProtocolFactory` core; `AnthropicAdapter`
      logic internalized as the `MessagesProtocolFactory` core (as core impls, not wrappers).
- [ ] `DeepSeekAdapter`, `VolcengineAdapter`, `OpenRouterAdapter`, `MinimaxAdapter` and their
      `*Config` types removed; behavior now = protocol core + the provider's profile.
- [ ] DeepSeek / Volcengine / Minimax are pure `ProviderEntry` (+ profile) over Chat/Messages
      factories, matching ADR Phase 3 item 3.
- [ ] Per-provider request/response assertion suites
      (`{anthropic,openai,deepseek,volcengine,openrouter,minimax}`) all green — request bodies,
      reasoning replay, usage interpretation, role mapping, path overrides identical to before.
- [ ] `grep` confirms the removed adapter/config types are gone.
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` pass.

## Notes

Breaking at the type level (the `*Adapter` / `*Config` types are public). The invariant to
hold: no behavior change — the profiles carry every divergence that the deleted adapters
used to encode. If any behavior cannot be expressed as protocol core + a named profile hook,
that is a signal the profile hook surface is incomplete; surface it as an ADR discussion
rather than reintroducing an adapter or a provider conditional (ADR "Provider profiles"
rules 1 and 3). The `lib.rs` re-exports of these types are cleaned up in issue 003.
