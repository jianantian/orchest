# v0.12 PRD: ADR-0002 Phase 3 — Remove Legacy Adapter/Factory Layer

## Background

[ADR-0002](../../adr/0002-protocol-provider-decoupling.md) decouples wire protocol from
provider identity via a three-layer model (protocol core + provider entry + provider
profile) across three migration phases. Phases 1–2 are non-breaking and land in
[hotfix 2026-07-12](../../hotfix/2026_07_12/prd.md): the protocol factories, provider
profiles, catalog-as-capability-source, the `provider/[protocol/]model` grammar, and the
Elss dissolution. After that hotfix the system is in a deliberate **transitional state**:

- All six LLM providers construct through protocol factories, but the legacy
  `ProviderFactory` trait and the per-provider `*Factory` structs still exist, kept alive
  by a bridge in `create_adapter_from_config`.
- The protocol factories in Phase 1 *wrap* the existing `*Adapter` logic, so the concrete
  `OpenAiAdapter` / `AnthropicAdapter` / `DeepSeekAdapter` / `VolcengineAdapter` /
  `OpenRouterAdapter` / `MinimaxAdapter` types (and their `*Config`) are still present and
  publicly re-exported.

v0.12 removes this transitional scaffolding: delete the legacy factory layer, collapse the
redundant adapter structs into the protocol cores, and leave the registry storing only
`ProviderEntry`. This is ADR-0002's Phase 3.

## Why this is a v0.x refactor iteration, not v1.0

Phase 3 is **breaking** — it removes public types (`ProviderFactory`, the `*Factory` and
`*Adapter`/`*Config` structs, their re-exports). Pre-1.0 versions may still break the public
API freely; **v1.0 is the freeze point**. Doing this cleanup *as* v1.0 would mean breaking at
the moment of freezing. So it lands as its own refactor iteration **before** v1.0. The
roadmap sequences it v0.11 → **v0.12** → v1.0, and the v1.0 freeze depends on it.

## Goals

1. Remove the legacy `ProviderFactory` trait, its bridge, and the six `*Factory` structs;
   the registry stores `ProviderEntry` only.
2. Collapse the redundant `*Adapter` structs so each provider's behavior is fully expressed
   by "protocol core + `ProviderEntry` + profile" — no vendor-named adapter types.
3. Clean up the public re-export surface and ship a v1.0-oriented migration note; keep the
   wall (`orchest-provider`) and consumers (`core`/`node`/`py`) naming only protocol + wall,
   never impl types.

## Non-Goals

- **No redesign of the three-layer model.** The shapes of protocol core / entry / profile
  are fixed by the hotfix; v0.12 only removes scaffolding.
- **`Protocol::Responses` still not implemented** — identifier only, pending its separate
  design review (stateful semantics), consistent with the hotfix.
- **No new provider or protocol.** Pure removal/consolidation.

## Issue Breakdown

| Issue | Title | Blocked by | Breaking surface |
|-------|-------|-----------|------------------|
| 001 | Remove legacy `ProviderFactory` trait, bridge, and `*Factory` structs | hotfix 001–010 | `ProviderFactory`, `ProviderRegistry` shape |
| 002 | Collapse redundant `*Adapter` structs into protocol cores | 001 | `*Adapter` / `*Config` public types |
| 003 | Public re-export cleanup + v1.0 migration note + binding sync | 002 | `lib.rs` re-exports |

Sequence is linear: 001 → 002 → 003. Each keeps the suite green; the breakage is to
compile-time public API, not runtime behavior (provider request/response behavior is
identical throughout, guarded by the existing per-provider test suites).

## Dependencies

- **Hard prerequisite:** hotfix 2026-07-12 fully merged (all six providers on protocol
  factories). Removing the legacy layer before every provider is migrated would break
  construction.
- **Gates v1.0:** the v1.0 freeze depends on v0.12 completing, so the public provider surface
  frozen at v1.0 is the post-Phase-3 one (entry + protocol, no `*Factory`/`*Adapter`).

## Acceptance Criteria

- [ ] 001–003 acceptance criteria all pass.
- [ ] `grep` finds no `ProviderFactory` trait, `*Factory` struct, or bridge dispatch remaining.
- [ ] DeepSeek / Volcengine / Minimax are pure `ProviderEntry` (+ profile) over Chat/Messages
      factories, with no separate adapter implementation (ADR Phase 3 item 3).
- [ ] Every provider's request/response behavior unchanged (per-provider test suites green).
- [ ] Wall and consumer crates name only protocol + wall; no impl type leaks.
- [ ] `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` /
      `cargo fmt --check` / `bash scripts/lint-check.sh` all pass.

## Verification

Each issue is verified by the existing per-provider request/response assertion suites
(`{anthropic,openai,deepseek,volcengine,openrouter,minimax}` tests) staying green across the
removal, plus a `grep` sweep confirming the deleted symbols are gone. Because behavior is
unchanged, no live-provider run is required for v0.12 itself (it inherits the hotfix's
coverage); a live smoke test is still owed at the v1.0 gate per the v0.10 report.
