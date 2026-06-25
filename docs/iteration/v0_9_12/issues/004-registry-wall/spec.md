# Issue 004: Registry + umbrella wall (orchest-providers)

## Background

Today there are **two** registry shapes — LLM `ProviderRegistry` (factory-by-name, returns
`Box<dyn ModelAdapter>`, `…/providers/src/registry.rs`) and ASR `AsrRouter`
(`register_provider` + `select_for_streaming`/`select_for_transcribe`, `asr/src/routing.rs:37`). Neither
generalizes. This issue builds `orchest-providers`, the consumer wall: a multi-capability,
descriptor-queryable registry + vendor facade + feature flags, reconciling both existing shapes and reusing
`AsrRouter.select_for_*` as the "pick" prototype.

## Goal / Scope

Build the wall and its selection mechanism.

In scope:

- Multi-capability registry keyed by (capability, provider, model), querying the **static descriptor core**.
- Selection: capability-query + identity-pick in **one** mechanism, mixable; pick-one + list-then-choose
  (PRD Decision 4 constraints). Settle the exact builder surface here against real call sites.
- Vendor-namespaced facade (`providers::volcengine::{chat,asr,tts}`).
- Feature flags (`volcengine`/`llm`/`asr`/…) mapping to impl-crate features; **cfg-gated registration**
  that compiles with any feature subset (no reference to a disabled impl crate).

Out of scope:

- Provider impls themselves (Issues 005–007) — the wall lands with LLM first; others wire in as they migrate.

## Acceptance Criteria

- [ ] `orchest-providers` exposes a registry serving capability-query **and** identity-pick as one
      mechanism; the two mix (e.g. "bidirectional ASR from Volcengine").
- [ ] `AsrRouter.select_for_*` logic is reused, not re-invented; `ProviderRegistry` factory-by-name is folded in.
- [ ] cfg-gated registration compiles with any feature subset; no path names a disabled impl crate.
- [ ] The vendor facade re-exports without the consumer naming an impl crate.
- [ ] The chosen selection API surface is recorded (PRD Decision 4) with the call sites that justified it.

## Notes

Depends on Issue 002 (descriptor) + Issue 003 (core). The exact selection surface is decided here, against real call sites.
