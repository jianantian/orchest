# ADR-0001: Provider Unification — Capability-Oriented Architecture

**Status:** Accepted (registry API surface deferred — see Decision 4)
**Date:** 2026-06-26
**Deciders:** Orchest maintainer (emile)
**Related:** [`docs/iteration/v0_9_12/prd.md`](../iteration/v0_9_12/prd.md), [`docs/todo/provider-unification.md`](../todo/provider-unification.md)

## Context

Provider code is split into five crates by **modality**: `agent-runtime-providers` (LLM),
`agent-runtime-aigc-providers` (image/video/music), `agent-runtime-asr-providers`,
`agent-runtime-tts-providers`, and the newly added `agent-runtime-realtime-providers`.
Two kinds of evidence show this split is not a real architecture boundary:

- **Vendor spread (measured).** Volcengine is re-implemented across **5 crates**, Minimax across
  **3**, each re-writing that vendor's HTTP/auth/protocol/client. Infra is duplicated: per-crate
  `http.rs`, two near-identical `observability.rs` (asr 259 / tts 239 LOC), a separate `sse/mod.rs`
  (353 LOC) and `telemetry.rs` (182 LOC).
- **Second divergence.** `agent-runtime-realtime-providers` does not depend on `agent-runtime-model`;
  it re-invented `RealtimeError` and a provider-local event enum instead of reusing the shared
  `ContentBlock`. Left alone it becomes a fifth independent infra stack.

Inspecting endpoints reveals the axis the modality split missed: **what drives code reuse is the
wire protocol (API dialect), not the vendor and not the modality.** Volcengine's LLM is
OpenAI-compatible (`ark.cn`) and shares a wire format with OpenAI/DeepSeek/OpenRouter — *not* with
Volcengine's own openspeech-WS ASR/TTS/realtime. Minimax is the opposite: one dialect
(`api.minimaxi.com`) spans LLM, TTS and music. The new omni realtime path is the *same* openspeech
binary WS already implemented (differently) for ASR/TTS.

**Forces at play:**
- Kill the duplicated auth/client/infra without building a god-trait.
- Two consumer types must both be served: capability-driven ("a thinking model that accepts
  image/video") and identity-driven ("Volcengine ASR", "OpenAI gpt-5.4").
- A pure-LLM consumer must not compile websocket/OSS dependencies (dependency-weight isolation).
- The split must seat two acceptance rulers without provider-local escape hatches: **omni**
  (full-duplex, audio+text out, mid-stream tool use) and **Chameleon** (turn that natively emits image).
- ASR/TTS are already published satellite crates; LLM (`agent-runtime-providers`) is the only one with
  internal consumers (`core/node/py`).

## Decision

Adopt a **capability-oriented architecture organized along two independent dimensions, decoupled by a
registry/umbrella wall** (Option D). Neither dimension leaks into the other:

- **Consumer side:** select by capability or by identity through one wall (`orchest-providers` +
  `orchest-protocol`); impl crates and wire dialects are never named.
- **Implementation side:** four reusable layers — protocol spine (`orchest-protocol`), building
  blocks + auth strategies (`orchest-provider-core`), wire-dialect implementations, and thin
  per-provider entries. Crate boundaries exist *only* to isolate dependency weight; wire dialects are
  modules inside the weight-tier crates.

Concrete decisions recorded by this ADR:

1. **Version slot: v0.9.12** (refactor iteration, provider satellite line).
2. **Naming prefix: `orchest-*`.** Crates: `orchest-protocol` (evolved from `agent-runtime-model`),
   `orchest-provider-core`, `orchest-provider-http` / `-stream` / `-visual`, `orchest-providers`
   (umbrella). Renaming the non-provider crates (`core/node/py`) is a follow-up, not a blocker.
3. **Impl-crate granularity: by dependency weight** (`http` = REST/SSE, `stream` = WebSocket,
   `visual` = signed/polled gen), with wire dialects as modules inside. Determined by the boundary
   principle below, not by taste.
4. **Registry selection API surface: deferred to Issue 004** (does not block Phase 1). Constraints are
   fixed now (one mechanism serves capability-query and identity-pick, allows mixing them, supports
   pick-one and list-then-choose, never exposes an impl crate). Exact surface is settled against real
   call sites during implementation; starting direction is a fluent builder.
5. **Event & descriptor shape: common core + typed extensions** (not a god-enum / god-struct). The event
   core keeps the existing `StreamEvent` **delta granularity** — one vocabulary for turn and duplex, with
   `AudioDelta`/`Transcript`/`Lifecycle` added and ASR-specific events as a typed extension; delivery
   unifies on a pulled `events()` stream (today turn pushes via `mpsc`, ASR pulls). The descriptor core
   holds only what the registry queries and must be available **statically** (catalog data) so the registry
   can filter before instantiating a provider.

## Options Considered

### Option A: One crate per vendor
All of a vendor's modalities in one crate (`orchest-provider-volcengine`, …), modalities behind features.

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium |
| Duplication killed | **No** — re-clusters by the wrong thing |
| Dependency weight isolation | Via intra-crate features only |
| Team familiarity | High (matches "one SDK per vendor") |

**Pros:** Maps to how vendors ship; intuitive vendor view.
**Cons:** **Premise is false.** A vendor is not one wire client — Volcengine is three (ark=OpenAI-compat,
openspeech-WS, visual-signed). Bundles code that does *not* share a client (volc-ark ↔ volc-asr) and
splits code that does (volc-ark ↔ openai). God-crate with a large feature matrix per vendor.

### Option B: By dependency weight only (the original todo note)
`core` + turn/duplex/asset crates; providers slotted by transport.

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium |
| Duplication killed | Partially |
| Dependency weight isolation | **Yes** (its whole point) |
| Vendor spread | **Re-introduced** |

**Pros:** Cleanest dependency-weight isolation at the crate level.
**Cons:** Re-fragments each vendor across turn/duplex/asset crates; the shared vendor auth/client has
no home (back to duplication or core knowing every vendor). Solves the symptom, not the vendor-spread cause.

### Option C: Single mega-crate + features
One `orchest-providers` crate, vendors and modalities all feature-gated.

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low (plumbing) / High (feature matrix) |
| Duplication killed | Yes |
| Dependency weight isolation | Feature-gated, weak at crate level |
| Boundaries | **Weakest** — everything sees everything |

**Pros:** Fewest crates; single version to bump.
**Cons:** No compile-unit isolation; feature matrix grows unbounded; internal boundaries unenforceable.

### Option D: Two dimensions, decoupled by a wall (CHOSEN)
Consumer selects by capability/identity through a wall; implementation organized by wire-dialect inside
dependency-weight crates; the shared protocol spine is reused everywhere.

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium |
| Duplication killed | **Yes** — one auth/client per wire dialect |
| Dependency weight isolation | **Yes** — weight tiers are the crate seams |
| Consumer ergonomics | **Both** capability and identity views, impl hidden |
| Seats omni + Chameleon | **Yes** — one unified event model + content model |

**Pros:** Kills duplication at the *right* unit (wire dialect); satisfies both doc goals at once
("share one client per vendor where real" + "crate boundary = dependency weight"); serves both consumer
types; absorbs the realtime crate; light consumers stay light.
**Cons:** More moving parts than C; requires getting the capability-descriptor vocabulary right;
cross-workspace Cargo feature unification needs care.

## Trade-off Analysis

The decisive observation: **vendor and modality are one-dimensional projections of a two-dimensional
problem.** Slicing by vendor duplicates capability/transport code across vendor crates; slicing by
modality (today) or by dependency-weight-only duplicates vendor auth/client across capability crates.
Options A and B each pick one axis and pay duplication on the other.

The **wire dialect** is the unit where three concerns coincide: dependency weight (OpenAI-compat=`reqwest`;
openspeech=`tokio-tungstenite`; signed-gen=`hmac`/OSS), capability primitive (turn/duplex/gen-task), and
actual code reuse (one adapter per dialect). That coincidence is why it is the correct *implementation*
seam. Vendor and modality become labels/config (L3 entries) over dialect + capability.

Separately, **consumer ergonomics must not inherit the implementation layout** — that was the valid
objection to a pure dialect split. A registry/umbrella wall reconstructs both a capability view and a
vendor view on top of the dialect-organized implementation, so neither consumer type sees a dialect crate.

On Decision 3 (granularity), the boundary principle "a crate exists only to isolate dependency weight"
is *dispositive*: per-dialect crates (Option-A-like) would split crates whose dependencies are identical
(openai-compat, anthropic, minimax-rest are all just `reqwest`); a single impl crate (Option C) loses
compile-unit isolation. Only weight-tier crates with dialect modules satisfy the principle.

## Consequences

**Easier:**
- Each vendor's auth/client per dialect is written once; Volcengine 5→shared, Minimax 3→shared.
- Omni reuses `ContentBlock`/`ToolUse` and the unified event model; `agent-runtime-realtime-providers` is deleted.
- A pure-LLM consumer (`features = ["llm"]`) compiles no websocket/OSS code.
- Both consumer types are served through one entry point.
- `core/node/py` depend on providers only through `orchest-protocol` + `orchest-providers` (dependency inversion).

**Harder / to watch:**
- Cross-workspace Cargo feature unification — internal examples must keep the default build light.
- The `node`/`py` binding cutover touches **two** points — the `core::model::ModelAdapter` trait alias **and** the `agent_runtime_providers::create_adapter_from_config`/`normalize_provider_model` functions; deprecated re-exports must bridge both, then a breaking removal.
- The capability-descriptor vocabulary is now the consumer contract; incompleteness forces escape hatches. The two acceptance rulers are its completeness test.

**To revisit:**
- Decision 4 — the registry selection API surface — settled against real call sites in Issue 004.
- Whether `agent-runtime-core/node/py` are renamed to `orchest-*` in a later pass.

## Action Items

Tracked as the v0.9.12 issue breakdown (see PRD §Issue Breakdown):

1. [x] 001 — This ADR + capability-descriptor vocabulary; prove omni + Chameleon seat in it. (design note: `docs/iteration/v0_9_12/issues/001-protocol-design/design.md`)
2. [ ] 002 — `orchest-protocol` spine (capability traits incl. new `RealtimeSession`/`GenTask`; one delta-granular event model + `SessionInput` + typed extensions; static descriptor core; unified error).
3. [ ] 003 — `orchest-provider-core` extraction (one HTTP/SSE/WS/OSS/auth/telemetry stack).
4. [ ] 004 — Registry + umbrella wall: reconcile `ProviderRegistry` + ASR `AsrRouter` (reuse `select_for_*`) into one multi-capability, descriptor-queryable registry; settle the selection API surface here.
5. [ ] 005 — LLM migration + dependency inversion; deprecated re-export for `agent-runtime-providers`.
6. [ ] 006 — Stream impl + realtime absorption; delete `agent-runtime-realtime-providers`.
7. [ ] 007 — Visual + remaining modality migration (aigc, TTS).
8. [ ] 008 — Deprecation cleanup, bindings, `cargo tree` weight checks.

**Hard acceptance:** the architecture is rejected if it cannot seat **omni** (full-duplex, audio+text
out, mid-stream tool use) and **Chameleon** (turn natively emitting image) without provider-local
content/event types.
