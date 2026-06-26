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
- Vendor-namespaced facade **scaffold** (`providers::volcengine::{chat,asr,tts}`) — the structure and
  re-export pattern; concrete per-vendor rows are added by the impl issues.
- The **cfg-gated registration mechanism** + feature-graph **scaffold**; it must build with **zero**
  registered impls (mechanism-only). Concrete `volcengine`/`llm`/`asr` feature rows and registrations are
  added by Issues 005/006/007, not here.
- Create the **empty impl-crate skeletons** `orchest-provider-{http,stream,visual}` (Cargo.toml + lib stub +
  workspace members + feature stubs) so the impl issues only add modules — removing the crate-creation race.

Out of scope:

- Provider impls **and** their concrete registrations / per-vendor feature+facade rows — those land in
  Issues 005/006/007. This issue ships the mechanism + facade skeleton, unit-tested against fixture descriptors.

## Acceptance Criteria

- [ ] `orchest-providers` exposes a registry serving capability-query **and** identity-pick as one
      mechanism; the two mix (e.g. "bidirectional ASR from Volcengine").
- [ ] `AsrRouter.select_for_*` logic is reused, not re-invented; `ProviderRegistry` factory-by-name is folded in.
- [ ] The registration mechanism builds with **zero** impls registered (mechanism-only build is green); cfg
      subsets compile with no path naming a disabled impl crate.
- [ ] The facade scaffold compiles with zero registered impls; concrete per-vendor re-exports are deferred to the impl issues.
- [ ] Selection is unit-tested against **fixture descriptors** (no real impl crate required).
- [ ] The chosen selection API surface is recorded (PRD Decision 4) with the call sites that justified it.

## Notes

Depends on Issue 002 (descriptor) + Issue 003 (core). The exact selection surface is decided here, against real call sites. Concrete provider registrations land in Issues 005/006/007.
