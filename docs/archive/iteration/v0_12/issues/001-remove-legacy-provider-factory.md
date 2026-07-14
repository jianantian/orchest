# 001 · Remove legacy `ProviderFactory` trait, bridge, and `*Factory` structs

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 3 · [v0.12 PRD](../prd.md)

## Background

After the hotfix 2026-07-12 (ADR-0002 Phase 1–2), all six LLM providers construct through
protocol factories, but the legacy `ProviderFactory` trait, the per-provider `*Factory`
structs (`AnthropicFactory`, `OpenAiFactory`, `DeepSeekFactory`, `OpenRouterFactory`,
`VolcengineFactory`, `MinimaxFactory`, `ElssFactory`), and the bridge in
`create_adapter_from_config` are all still present — kept only to route during the migration.
With every provider migrated, this layer is dead weight.

## Goal

Delete the legacy factory layer. `ProviderRegistry` stores `ProviderEntry` only; the
`create_adapter` / `create_adapter_from_config` free functions resolve purely through the
protocol factories (Chat/Messages) selected from the resolved provider entry + protocol.

## Acceptance Criteria

- [ ] `ProviderFactory` trait removed.
- [ ] All six `*Factory` structs removed.
- [ ] The bridge dispatch in `create_adapter_from_config` removed; construction goes through
      protocol factories only.
- [ ] `ProviderRegistry` stores `ProviderEntry` (descriptor + protocols + aliases + path
      overrides + headers + profiles); no factory objects.
- [ ] `create_adapter` / `create_adapter_from_config` public signatures preserved (they take
      model + config and return `Box<dyn ModelAdapter>`); only the internal resolution path
      changes.
- [ ] `grep` confirms no `ProviderFactory` / `*Factory` / bridge remains.
- [ ] All six providers' request/response behavior unchanged — per-provider test suites green.
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` pass.

## Notes

This is breaking at the type level (`ProviderFactory` and `*Factory` are public), which is
why it lives in a pre-1.0 refactor iteration. Runtime behavior must not change: the same
model strings resolve to the same adapters. The `*Adapter` structs are **not** removed here —
that is issue 002. This issue removes only the factory layer above them.
