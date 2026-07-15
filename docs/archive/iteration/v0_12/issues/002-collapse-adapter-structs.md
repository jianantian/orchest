# 002 · Collapse redundant `*Adapter` structs into protocol cores

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 3 · [v0.12 PRD](../prd.md)

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

## Settled design (source of truth)

Implementing the collapse revealed that the four Chat adapters' pre-flight handling of
reasoning / thinking-budget / reasoning-output-exclusion **differs per provider specifically
under `CompatibilityPolicy::Strict`** — broader than the initial "a few DeepSeek assertions"
framing:

| unsupported option, **Strict** | OpenAI | DeepSeek | Volcengine | OpenRouter |
|---|---|---|---|---|
| reasoning on non-reasoning model | errors `unsupported_reasoning_model` | n/a (supported) | silently disables (no error) | supported |
| `thinking_budget` | errors `unsupported_thinking_budget` | adjusts, no error | adjusts, no error | supported (uses it) |
| reasoning output exclusion | supported (moot) | errors `unsupported_reasoning_output_exclusion` | errors (same) | supported |

**Decision (agreed): byte-identical collapse via data-driven `option_support`.** Each
provider's exact Strict/degrade behavior and reason strings are carried as **data** on the
profile, so the single shared Chat core reproduces every provider byte-for-byte with **zero
provider-name conditionals**.

Concretely:

- `RequestOption` gains `Reasoning` and `ThinkingBudget` (alongside `ReasoningOutputExclusion`).
- `OptionSupport::Unsupported { strict_error: Option<(code, message)>, degrade: Option<Degrade> }`,
  where `Degrade { option, applied: AppliedValue, reason, disables_thinking }` encodes the exact
  `OptionAdjustment`. `strict_error: None` means Strict degrades silently (Volcengine disabling
  thinking on an unlisted model raises no error); `degrade: None` means degrade silently.
- A shared `resolve_chat_preflight(profile, cx, options) -> Result<(RequestOptions,
  Vec<OptionAdjustment>), ModelError>` applies reasoning/budget/exclusion uniformly. The
  reasoning/budget/exclusion adjustments **move out of `lower_options` into the pre-flight**;
  `lower_options` keeps only body-shaping (reasoning-effort / thinking dialect / `reasoning`
  object / sampling) and the provider's cache-policy adjustment.
- New hooks: `capabilities(cx, max_output_tokens) -> ModelCapabilities` (default =
  catalog-driven `canonical_chat_capabilities`; OpenAI/DeepSeek/Volcengine/OpenRouter override
  for their effort lists / budget / exclusion / replay-metadata / pricing / prefix fallback);
  `chat_sse_reasoning(cx) -> (Option<&str>, Option<&str>)` for the SSE decoder's reasoning
  field names (DeepSeek `reasoning`, Volcengine `reasoning_content`, OpenRouter
  `reasoning` + `reasoning_details`).
- `replay_reasoning` becomes **fallible** (`Result<(), ModelError>`) so OpenRouter's invalid
  `reasoning_details` replay keeps erroring (`invalid_reasoning_replay`).
- Stop-reason: a canonical `normalize_chat_stop_reason` folds the extra
  `insufficient_system_resource -> Interrupted` mapping into the shared post-processing (a
  no-op for OpenAI/OpenRouter, which never emit it).
- Every Chat provider now has a profile (OpenAI gains `OpenAiProfile`, carrying its
  `openai_pricing` + name-prefix context fallback in `capabilities`). The shared Chat core
  always resolves a profile from the entry, falling back to a `CanonicalChat` no-op profile.

### Messages core (Anthropic + Minimax) — settled design

The Messages core (`src/messages.rs`, `MessagesAdapter`) collapses `AnthropicAdapter` and
`MinimaxAdapter` into one shared adapter carrying `entry` / `catalog` / `profile`, mirroring the
Chat core. The canonical envelope lives in the core: the top-level `system` field, message /
content mapping, the `thinking: {type, display}` + `output_config` adaptive-vs-`budget_tokens`
dialect, cache control, sampling, and `service_tier` pass-through, plus the shared SSE consumer
(`src/messages/response.rs`, moved verbatim from `anthropic/response.rs`). There is no Strict
pre-flight divergence, so — unlike the Chat core — Messages needs no `resolve_*_preflight`; the
one Strict rule (`thinking_budget_tokens` under adaptive → `unsupported_thinking_budget` error)
stays inline in `complete`, gated by the profile's adaptive hook.

Provider divergence rides on four Messages hooks on `ProviderProfile`, all resolved by name (no
provider conditionals in the core):

- `messages_wire_role(cx, role, &mut adjustments) -> &'static str` — the default **is** Anthropic
  (drop Minimax-only roles to `user` with `minimax_only_role_dropped`); Minimax overrides to emit
  its native `user_system` / `group` / `sample_message_*` with no adjustment. (Replaces the old
  `map_role` + `WireRole`.)
- `encode_multimodal_block(cx, block, &mut adjustments) -> Option<Value>` — the ADR rule-4 fork.
  Both providers keep the same content-block *envelope*; only Image / Video / Audio /
  MidConvSystem encoding differs, so this stays a profile hook (a narrow encoder), **not** a new
  protocol. Anthropic: Image only, drop the rest with `anthropic_unsupported_content_block`.
  Minimax: Image (+`detail`) / Video (+`fps`/`max_long_side_pixel`) / MidConvSystem serialized,
  Audio dropped with `minimax_audio_block_unsupported_in_llm_api`.
- `messages_supports_adaptive(cx) -> bool` — Anthropic `fable-5`/`mythos-5`/`opus-4`/`sonnet-4`;
  Minimax `MiniMax-M3`. Gates both the thinking dialect and the Strict budget check.
- `messages_auth_headers(cx, api_key) -> Vec<(&'static str, String)>` — Anthropic
  `x-api-key` + `anthropic-version` + `content-type`; Minimax `Authorization: Bearer` +
  `content-type`.

`capabilities(cx, max_output_tokens)` (the shared Chat/Messages hook) carries each provider's
effort list / `budget_tokens` / context window / pricing (`anthropic_pricing`; Minimax zero
placeholder). Usage-missing handling is the shared default `interpret_usage`
(`usage_not_reported` + telemetry) — byte-identical for both. `service_tier` moved from
Minimax's old `lower_options` into the core: it is only emitted when the caller sets it, so
Anthropic stays byte-identical while Minimax keeps forwarding it. Anthropic gains an
`AnthropicProfile` (it is the canonical reference but still needs its capability / auth / adaptive
/ encoding facts as profile data); the Anthropic entry now declares that profile rather than
`profiles: &[]`. Elss's Messages branch builds the shared `MessagesAdapter` with Anthropic's
entry + profile at Elss's base URL, exactly as its Chat branch does with OpenAI. `messages.rs`
is a file module (not `mod.rs`) so it is exempt from the ≤150-line `mod.rs` rule, matching
`chat.rs`.

The one intentional non-byte-identical detail: the OpenAI reasoning-unsupported **error message**
loses its interpolated model name (becomes static); only the error *code* is asserted, and the
`provider`/`model` context is still on the `ModelError`.

## Notes

Breaking at the type level (the `*Adapter` / `*Config` types are public). The invariant to
hold: no behavior change — the profiles carry every divergence that the deleted adapters
used to encode. If any behavior cannot be expressed as protocol core + a named profile hook,
that is a signal the profile hook surface is incomplete; surface it as an ADR discussion
rather than reintroducing an adapter or a provider conditional (ADR "Provider profiles"
rules 1 and 3). The `lib.rs` re-exports of these types are cleaned up in issue 003.
