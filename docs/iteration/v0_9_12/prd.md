# v0.9.12 PRD: Provider Unification — Capability-Oriented Provider Architecture

> Type: **Refactor iteration (大重构)**. Version slot: **v0.9.12**.
> This PRD is the Step 3 design that `docs/todo/provider-unification.md` deferred,
> consuming the evidence from v0.9.10 (Minimax cross-crate) and v0.9.11 (omni realtime).
>
> Structure: **Background** (why now) → **Starting Point** (the real code we depart from) →
> **Target** (the idealized architecture we aim for) → **From Here To There** (the reasoned
> action path that connects them). The target is intentionally idealized; the path is
> intentionally incremental and grounded in the types that exist today.

## Background

Provider code is split into five crates by **modality**: `agent-runtime-providers` (LLM),
`agent-runtime-aigc-providers` (image/video/music), `agent-runtime-asr-providers`,
`agent-runtime-tts-providers`, and the newly added `agent-runtime-realtime-providers`.
Two independent kinds of evidence show this split is not a real architecture boundary:

1. **Vendor spread (measured).** Volcengine is re-implemented across **5 crates**, Minimax across
   **3**, each re-writing that vendor's HTTP/auth/protocol/client.
2. **The realtime crate is diverging a second time.** `agent-runtime-realtime-providers` does **not**
   depend on `agent-runtime-model`; it re-invented `RealtimeError` and a provider-local event enum
   instead of reusing the shared `ContentBlock`.

The axis the modality split missed: **what drives code reuse is the wire protocol (API dialect), not
the vendor and not the modality.**

| Vendor | turn (REST/SSE) | duplex (WS) | gen-task (sign/poll) |
|---|---|---|---|
| anthropic | Anthropic Messages | — | — |
| openai / deepseek / openrouter | OpenAI-compatible | — | — |
| **volcengine** | **OpenAI-compatible** (`ark.cn`, Bearer) | **openspeech binary WS** (asr/tts/realtime) | volc-visual signed (image/video) |
| **minimax** | minimax REST (`api.minimaxi.com`, Bearer) | minimax WS (tts) | minimax REST (music) |
| aliyun / deepgram / soniox / … | — | per-vendor ASR dialects | aliyun-OSS etc. |

Volcengine's LLM (`crates/agent-runtime-providers/src/providers/volcengine/mod.rs:1` — annotated
"OpenAI-compatible Chat Completions") shares a wire format with OpenAI, *not* with Volcengine's own
openspeech ASR/TTS/realtime. Minimax is the opposite: one dialect spans LLM/TTS/music. The omni
realtime path is the *same* openspeech binary WS already implemented for ASR/TTS. So `asr/tts/llm/aigc`
are **provider illustrations, not boundaries**; the reuse boundary is the **wire dialect**, which is
also where dependency weight and capability primitive agree.

## Starting Point — current code state (出发点)

The refactor departs from these concrete artifacts. Everything below is what exists **today**; the
action path transforms each into its target form. (Anchors are `path:line`.)

### Capability traits — three exist, two are missing

| Capability | Trait today | Location | Note |
|---|---|---|---|
| turn / chat | `ModelAdapter` | `agent-runtime-model/src/adapter.rs:17` | `complete(messages, tools, options, tx: Option<Sender<StreamEvent>>)` |
| asr | `AsrProvider` | `agent-runtime-asr-providers/src/traits.rs:10` | `transcribe` + `start_stream` |
| tts | `TtsProvider` (+ `VoiceManager`) | `agent-runtime-tts-providers/src/traits.rs:12,39` | `synthesize` + `stream` + `start_duplex_stream` |
| **realtime / omni** | **none** | — | concrete `VolcengineRealtimeSession` (`…/realtime/mod.rs:249`) — no trait |
| **gen-task** | **none** | — | concrete `ImageGateway::generate()` (`…/aigc/.../gateway/image/mod.rs:20,54`) — a gateway struct, not a provider trait |

### Streaming events — four divergent enums with different granularity

| Enum | Location | Granularity |
|---|---|---|
| `StreamEvent` | `agent-runtime-model/src/stream.rs:13` | **fine-grained delta** (`ToolUseStart`/`ToolUseArgsChunk`/`ToolUseEnd`, `ThinkingStart`/`Thinking`/`ThinkingEnd`, `Done{usage}`) |
| `AsrStreamEvent` | `agent-runtime-asr-providers/src/types.rs:354` | ASR-specific (`RouteSelected`, `TranscriptUpdate{stability}`, `EndOfSpeech`, `AsrFinal`) |
| `TtsStreamEvent` | `agent-runtime-tts-providers/src/streaming.rs:8` | TTS-specific |
| `VolcengineRealtimeEvent` | `…/realtime/mod.rs:156` | provider-local, not in the model crate at all |

### Capability descriptors — three structs **plus** a separate catalog layer; fields the consumer needs are split across them

| Descriptor | Location | Holds | Missing |
|---|---|---|---|
| `ModelCapabilities` | `agent-runtime-model/src/options.rs:87` | `streaming`, `tool_use`, `reasoning`, `prompt_cache`, pricing | **no input-modality fields** (can't express "accepts image/video") |
| `AsrModelCapabilities` | `agent-runtime-asr-providers/src/types.rs:430` | 20+ ASR-specific fields (endpointing, diarization, `connection_reuse`, …) | — |
| `TtsModelCapabilities` | `agent-runtime-tts-providers/src/types.rs:368` | TTS/voice-specific | — |
| `LlmModelEntry` / `Modality` / `ModelScene` / `ThinkingSpec` | `agent-runtime-providers/src/catalog/mod.rs:29,48,57,72` | the LLM **input-modality** info, model scene, thinking spec | lives in a *different layer* from `ModelCapabilities` |

A consumer query like `.accepts([Image,Video]).thinking()` needs the modality bit from the **catalog**
layer and the thinking bit from **`ModelCapabilities`** — two different structs in two different crates.
`CapabilitySource` (`Static/ProviderMetadata/Assumed`) is **literally duplicated** at
`agent-runtime-model/src/options.rs:39` and `agent-runtime-asr-providers/src/types.rs:416`.

### Errors, registry, pricing, infra, auth

- **Errors:** four — `ModelError` (`…/model/src/error.rs:21`), `AsrError` (`asr/error.rs:27`),
  `TtsError` (`tts/error.rs:27`), `RealtimeError` (`realtime/error.rs:20`).
- **Registry — two different shapes already exist.** LLM: `ProviderFactory` / `ProviderRegistry`
  (`…/providers/src/registry.rs:9,23`) — factory-by-provider-name, LLM-only (returns `Box<dyn ModelAdapter>`,
  LLM-shaped signature `max_tokens`/`api_key`/`api_url`), **hardcoded `register(...)` list**, **no descriptor
  query**. ASR: `AsrRouter` (`asr/src/routing.rs:37`) — instance-by-model with `register_provider()` +
  `select_for_streaming()`/`select_for_transcribe()`; its selection is the closest existing prototype of the
  capability "pick" we want. (TTS has a parallel router.)
- **How `node/py` actually obtain an adapter (two touch points).** They store the trait as
  `agent_runtime_core::model::ModelAdapter` (a `core` re-export of `agent_runtime_model`), **and** they
  construct it by calling the free functions `agent_runtime_providers::create_adapter_from_config` +
  `normalize_provider_model` (multiple call sites — enumerate with
  `rg "create_adapter_from_config|normalize_provider_model"`, not fixed line numbers). `agent-runtime-core` does **not**
  construct any provider registry. → migration must insulate **both** the trait path and these two functions.
- **Pricing:** three billing models — `ModelPricing` token-tier (`…/options.rs:110`), ASR
  duration-based (`AsrUsage.cost_estimate_micros`), aigc per-asset.
- **Infra duplication:** per-crate `http.rs`, two near-identical `observability.rs` (asr 259 / tts 239 LOC),
  separate `sse/mod.rs` (353 LOC) and `telemetry.rs` (182 LOC).
- **Auth:** header-based everywhere — `Authorization: Bearer` for REST; custom `X-Api-*` headers on the WS
  upgrade for openspeech (`…/realtime/live.rs:250`, `asr/.../volcengine/mod.rs:207`). No stateful crypto
  handshake; auth factors cleanly as a header-injection strategy.

## Target — idealized architecture (目标)

Organized along **two independent dimensions, decoupled by a registry/umbrella wall** so neither leaks:
the **consumer** selects by capability or identity and never names an impl crate or wire dialect; the
**implementation** is organized by what is reusable, with crate boundaries that exist only to isolate
dependency weight and wire dialects as modules inside.

### 1. The protocol spine — `orchest-protocol` (evolves `agent-runtime-model`)

- **Content model** — keep `ContentBlock` (`Text/Image/Video/Audio/Thinking/ToolUse/ToolResult`), already modality-complete.
- **Capability traits** — parallel, opt-in, no god-trait:
  `ChatModel` (← `ModelAdapter`), `Asr` (← `AsrProvider`), `Tts` (← `TtsProvider`);
  `VoiceManager` stays a **separate optional** trait (voice clone/design) parallel to `Tts`, not merged in;
  `RealtimeSession` (**new**, lifts the concrete realtime session), `GenTask` (**new**, lifts `ImageGateway`).
- **One streaming event model** — the most reasonable target is a single **content-event core** at
  `StreamEvent`'s existing **delta granularity** (keep `ToolUseStart/Chunk/End`, `Thinking*`), extended
  with the variants duplex/asr/tts need (`AudioDelta`, `Transcript{stability}`, `Lifecycle`,
  `Error`), plus **typed per-capability extension** where a modality has genuinely specific events
  (ASR segment/endpointing). One vocabulary replaces the four enums above; capability traits differ on the
  **input/session** side, not on the event vocabulary. The realtime/omni send side is
  `SessionInput { Audio | Text | ToolResult | Interrupt }`. Delivery is also unified: today turn pushes via
  `tx: Option<mpsc::Sender<StreamEvent>>` while ASR pulls via `AsrStream` — the target settles on one
  direction (a pulled `events()` stream); the push→pull move is part of this work, not a detail.

  > Reasonableness note: an earlier sketch used a coarse `SessionEvent::ToolUse{id,name,input}`. That
  > contradicts the existing delta-granular `StreamEvent`. The target keeps delta granularity so turn and
  > duplex share one event vocabulary rather than maintaining two.

- **One capability descriptor** — a **common queryable core** (input modalities, output modalities,
  `streaming`, `tools`, `thinking`, `duplex`/`realtime`, `interruptible`) that the registry filters on,
  **plus typed per-capability extensions** that preserve detail (`AsrModelCapabilities`'
  endpointing/diarization is **not** flattened). The catalog modality info (`LlmModelEntry`/`Modality`)
  folds into the common core; `CapabilitySource` is de-duplicated to one definition. The common core must be
  available **statically** (catalog data, like `LlmModelEntry` today) so the registry can filter **before**
  instantiating a provider; `CapabilitySource::{Static,ProviderMetadata,Assumed}` already encodes where a
  given field came from.
- **One error** — unify `ModelError`/`AsrError`/`TtsError`/`RealtimeError`.

### 2. Reusable implementation layers (L0–L3)

| Layer | Responsibility |
|---|---|
| **L3 provider entries** | a few lines per `(vendor × model × endpoint)`: dialect, auth scheme, base URL, descriptor entry |
| **L2 dialect implementations** | map `orchest-protocol` ↔ a wire format (OpenAI-compat, Anthropic, openspeech, minimax, per-vendor ASR, signed-gen) — **where reuse clusters** |
| **L1 auth strategies** | header-injection strategies: `Bearer`, `X-Api-*` header sets, `AK/SK-HMAC`, `OSS-signature`; an entry binds one per endpoint |
| **L0 building blocks** | http client, SSE, bidirectional WS scaffold, binary-frame codec, OSS upload, gen-task poller, retry, telemetry, catalog storage |

**One provider, multiple auth** is native: Volcengine = three L3 entries binding three header strategies
(ark→`Bearer`, openspeech→`X-Api-App-ID/Access-Key`, visual→`AK/SK-HMAC`).

### 3. Crate topology (target)

```
orchest-protocol          spine: ContentBlock + capability traits + unified event model +
                          unified capability descriptor + unified error  (from agent-runtime-model)
orchest-provider-core     L0 + L1 + cross-cutting (http/sse/ws/oss/auth/retry/telemetry/
                          pricing/catalog), gated by weight features (`ws`, `oss`, `sse`)
orchest-provider-http     L2/L3 REST+SSE dialects (openai-compat · anthropic · minimax-rest · rest-asr · music)
orchest-provider-stream   L2/L3 WS dialects (openspeech[asr/tts/omni] · minimax-ws · streaming asr)  ← absorbs realtime + asr/tts crates
orchest-provider-visual   L2/L3 signed/polled gen (volc-visual · aliyun · …)  ← absorbs aigc crate (music → http)
orchest-providers         THE WALL: registry + vendor facade + feature flags (re-exports impl crates)
```

### 4. Consumer-facing API (the wall)

Consumers depend on exactly `orchest-protocol` + `orchest-providers`. Three selection styles, all hiding
impl crates (**non-normative pseudocode** — the exact surface is settled in Issue 004; Rust has no arity
overloading, so `chat()` vs `chat("openai/gpt-5.4")` below is illustrative, not a literal signature):

```rust
use orchest_providers::registry;
let model = registry.chat().accepts([Text, Image, Video]).thinking().pick()?;  // by capability
let asr   = registry.asr().provider("volcengine").bidirectional().pick()?;     // capability + identity
let model = registry.chat("openai/gpt-5.4")?;                                  // by identity
let asr   = providers::volcengine::asr(my_config)?;                            // full control, vendor facade
```

`features = ["volcengine"]` / `["llm"]` / `["asr"]` control compiled weight without naming impl crates;
`["llm"]` pulls no websocket/OSS deps. The umbrella reconstructs a **vendor view**
(`providers::volcengine::{chat,asr,tts}`) over implementations that physically live in different dialect crates.

## From Here To There (行动路径)

The path is incremental and dependency-ordered. Each row names the real artifact, its target, and the move.

| Today (start) | Target | Transformation |
|---|---|---|
| `ModelError`/`AsrError`/`TtsError`/`RealtimeError` | one error | Define unified error in `orchest-protocol` first (lowest-risk, unblocks everything); old errors become `From`/alias shims. |
| `StreamEvent` + `AsrStreamEvent` + `TtsStreamEvent` + `VolcengineRealtimeEvent` | one delta-granular event model + typed extensions | Promote `StreamEvent` to the common core, add duplex/asr/tts variants, keep ASR-specific events as a typed extension; adapters map onto it. |
| `ModelCapabilities` + `AsrModelCapabilities` + `TtsModelCapabilities` + catalog `LlmModelEntry`/`Modality`; `CapabilitySource` ×2 | common descriptor core + typed extensions | Extract the queryable core; fold catalog modality info in; de-dup `CapabilitySource`; keep per-capability structs as extensions. |
| `ModelAdapter`/`AsrProvider`/`TtsProvider` | `ChatModel`/`Asr`/`Tts` in `orchest-protocol` | Move + rename (keep `ModelAdapter` as deprecated alias). |
| concrete `VolcengineRealtimeSession`; concrete `ImageGateway` | `RealtimeSession` / `GenTask` traits | **New abstractions** lifted from the concrete code (this is design work, not a move). |
| per-crate `http.rs`/`observability.rs`/`sse`/`telemetry`; header auth scattered | `orchest-provider-core` L0+L1 | Extract one stack; auth becomes header-injection strategies. |
| LLM `ProviderRegistry` (factory-by-name) **+** ASR `AsrRouter` (select-by-model) | one multi-capability, descriptor-queryable registry behind the wall | Reconcile **both**; reuse `AsrRouter.select_for_*` as the "pick" prototype. Today's LLM factory signature does not generalize → replace it, don't extend. |
| `node/py` use `core::model::ModelAdapter` **and** `agent_runtime_providers::{create_adapter_from_config, normalize_provider_model}` | unchanged trait path + unchanged construction fns during migration | Insulate **two** points: keep the `core::model` trait alias **and** preserve those two free functions as deprecated re-exports; flip `core` to `orchest-protocol` under the alias. |

### Phases

1. **Phase 1 — Spine.** Create `orchest-protocol` from `agent-runtime-model`: unified error, unified
   event model, common descriptor core + typed extensions, capability traits (`ChatModel`/`Asr`/`Tts`),
   and the new `RealtimeSession` + `GenTask` trait shapes. No provider behavior change; old types aliased.
   This is the highest-design-density phase — the two acceptance rulers are validated against the trait/event/descriptor shapes here.
2. **Phase 2 — Core.** Extract `orchest-provider-core` (L0 building blocks + L1 header-auth strategies),
   collapsing the duplicated `http`/`sse`/`telemetry`/`observability`.
3. **Phase 3 — Wall + LLM.** Build `orchest-providers` (multi-capability registry reconciling
   `ProviderRegistry` + `AsrRouter` + descriptor query + vendor facade). Move LLM providers behind it; keep
   `agent-runtime-providers` as a deprecated re-export preserving **both** the `core::model` trait alias and
   the `create_adapter_from_config`/`normalize_provider_model` functions `node/py` call; flip `core → orchest-protocol`.
4. **Phase 4 — Fold modalities + realtime.** Migrate asr/tts into `orchest-provider-stream` (openspeech,
   minimax-ws, per-vendor ASR); migrate aigc into `visual`/`http`; **absorb** the realtime crate as a
   `RealtimeSession` impl; reconcile the three pricing models in core.
5. **Phase 5 — Cleanup.** Remove deprecated re-exports; finalize feature graph; update `node/py`/examples;
   record `cargo tree` weight evidence.

The phases are stages, not a strict serial chain. Once the spine (1) and core (2) land, the impl crates
(LLM / stream / visual) need only spine+core to **build**, so they can be developed in parallel. But their
**registration into the wall** depends on the registry *mechanism* from Issue 004 — which itself needs only
spine+core and ships as a mechanism + facade skeleton, with each impl issue adding its own entries. So:
parallel build after (2); registration serializes behind 004's mechanism.

## Acceptance Test Cases (the two rulers)

The architecture is rejected if it cannot seat **both** without provider-local content/event types.

- **Omni full-duplex realtime call** — audio in / audio out / text out / **mid-stream tool calls**,
  concurrently. Lands as `RealtimeSession` (send `SessionInput`, receive the unified event stream;
  `ToolUse*` mid-stream, tool runs on a separate task, `ToolResult` sent back, audio never stops) in the
  `openspeech` dialect inside `orchest-provider-stream`.
- **Chameleon turn-emits-image** — a `ChatModel` whose output event stream carries `Image` (reusing the
  content model), no `GenTask` involved.

## Goals

1. Collapse vendor spread: each vendor's auth/client/dialect written once.
2. Kill infra duplication: one `http/sse/ws/telemetry/retry/catalog` stack.
3. Collapse the four event enums, three+catalog descriptors, four errors into one model each (core + typed extensions).
4. Absorb `agent-runtime-realtime-providers`; omni reuses `ContentBlock` + the unified event model.
5. One consumer entry point with capability-query **and** identity-pick.
6. No god-trait; dependency weight controllable (`features=["llm"]` → no ws/oss).
7. Seat both acceptance rulers in the protocol spine.

## Non-Goals

- Do not keep the modality crate names as the long-term surface (deprecated re-exports only).
- Do not build new provider integrations; this re-organizes existing code (plus the new `RealtimeSession`/`GenTask` abstractions over existing concrete code).
- No plugin/dynamic loading; impl crates are compiled-in behind features.
- Do not change wire protocols or provider behavior.
- Do not block on renaming `core/node/py`.

## Success Metrics

- Volcengine/Minimax: one auth/client per wire dialect, no cross-crate duplication.
- One event model, one descriptor core, one error — the four/three/four-way splits are gone; `AsrModelCapabilities` detail survives as a typed extension (not flattened).
- `agent-runtime-realtime-providers` deleted; omni runs as `RealtimeSession`.
- `features = ["llm"]` dependency tree has no `tokio-tungstenite`/OSS-signing (`cargo tree` evidence).
- `core/node/py` reach providers only through `orchest-protocol` + `orchest-providers`.
- Both rulers compile against the protocol with no provider-local content/event types.

## Risks

- **Descriptor flattening.** Forcing `AsrModelCapabilities` into a generic blob loses real detail. Mitigation: common core + typed extensions; the core is only what the registry queries.
- **Event-granularity reconciliation.** The four enums differ in philosophy (delta vs whole vs ASR-segment). The target fixes delta granularity as the core; ASR-specific events stay typed extensions.
- **Registry generalization.** Today's `ProviderFactory` is LLM-shaped and does not generalize — Phase 3 replaces it; the cfg-gated, descriptor-queryable, multi-capability registry is genuinely new work.
- **`GenTask`/`RealtimeSession` are net-new traits**, not moves — aigc uses a `Gateway` struct and realtime is a concrete session; abstracting them is design, sequenced in Phase 1 and validated by the rulers.
- **Pricing across three billing models** (token-tier / duration / asset) must be reconciled in core (Phase 4).
- **Binding cutover.** `node/py` depend on **two** things — the `core::model::ModelAdapter` trait alias and the `create_adapter_from_config`/`normalize_provider_model` free functions; both must be preserved as deprecated re-exports during migration. The eventual removal is breaking and updates both bindings + examples.
- **Cargo feature unification** across the workspace; internal examples must keep the default build light.
- **Big-bang temptation.** Five phases must land independently and stay green.

## Issue Breakdown

| Issue | Title | Scope |
|---|---|---|
| 001 | ADR + capability-descriptor & event reconciliation design | `docs/adr/0001`; design common descriptor core + typed extensions and the unified event model; prove omni + Chameleon seat |
| 002 | `orchest-protocol` spine | Unified error/event (delta core + extensions; pull on new traits, chat/asr/tts push→pull deferred to 005/006)/descriptor (with static catalog form); `ChatModel`/`Asr`/`Tts`; **new** `RealtimeSession`/`GenTask` shapes; alias old types |
| 003 | `orchest-provider-core` extraction | One http/sse/ws/oss/telemetry stack + L1 header-auth strategies |
| 004 | Registry + umbrella wall | Reconcile `ProviderRegistry` + `AsrRouter` into one multi-capability, descriptor-queryable registry (reuse `select_for_*`); vendor facade; cfg-gated feature wiring |
| 005 | LLM migration + dependency inversion | LLM providers behind the wall; deprecated re-export; insulate `node/py` at `core::model`; flip `core → orchest-protocol` |
| 006 | Stream impl + realtime absorption | openspeech/minimax-ws/asr dialects; delete `agent-runtime-realtime-providers`; omni as `RealtimeSession` |
| 007 | Visual + remaining modality migration | aigc image/video/music → `visual`/`http` (abstract `GenTask` from `ImageGateway`); reconcile pricing (token/duration/asset). ASR/TTS are owned by Issue 006. |
| 008 | Cleanup + bindings | Remove shims; finalize features; update `node`/`py`/examples; `cargo tree` weight checks |

Dependency order: 001 → 002 → 003 → 004 (registry mechanism + facade + empty impl-crate skeletons) →
{005, 006, 007} in parallel (each fills a skeleton and registers its own entries via the 004 mechanism) → 008.

## Decisions

Recorded here and in ADR-0001.

1. **Version slot — v0.9.12.**
2. **Naming prefix — `orchest-*`.** Renaming `core/node/py` is a follow-up, not a blocker.
3. **Impl-crate granularity — by dependency weight** (`http`/`stream`/`visual`, dialects as modules).
   Follows the adopted principle "a crate exists only to isolate dependency weight": weight tiers are the
   only legitimate seams; per-dialect crates would split identical-dependency code (openai-compat,
   anthropic, minimax-rest are all just `reqwest`); a single impl crate loses isolation and invites a
   feature matrix.
4. **Registry selection API surface — deferred to Issue 004** (does not block Phase 1). Constraints fixed:
   one mechanism serves capability-query and identity-pick and allows mixing; supports pick-one and
   list-then-choose; never exposes an impl crate. Starting direction (not final): a fluent builder with
   `chat("openai/gpt-5.4")` as the identity shorthand.
5. **Event & descriptor shape — common core + typed extensions** (not a god-enum / god-struct).
   Event core keeps `StreamEvent`'s delta granularity; descriptor core holds only what the registry queries.

## Acceptance Criteria

- [ ] `docs/adr/0001-provider-unification.md` records the decisions with rationale; omni + Chameleon are hard acceptance.
- [ ] `orchest-protocol` exposes unified error, one delta-granular event model (with typed extensions), one descriptor core (with typed extensions), and the capability traits incl. new `RealtimeSession`/`GenTask`; old `agent-runtime-model` types are aliased.
- [ ] `orchest-provider-core` holds a single http/sse/ws/telemetry/auth stack; per-crate `http.rs`/`observability.rs` duplicates and the duplicate `CapabilitySource` are gone.
- [ ] Each vendor's auth/client per wire dialect exists once; Volcengine's three endpoints are three L1 header strategies bound by L3 entries.
- [ ] `agent-runtime-realtime-providers` is deleted; omni runs as `RealtimeSession` reusing `ContentBlock`/`ToolUse`.
- [ ] `AsrModelCapabilities`-level detail survives as a typed extension (not flattened into the core).
- [ ] Consumers select via capability query **and** identity pick through `orchest-providers`; no consumer references an impl crate or dialect.
- [ ] `features = ["llm"]` yields a dependency tree with no `tokio-tungstenite`/OSS-signing (`cargo tree` evidence recorded).
- [ ] `node/py` reach providers only through `orchest-protocol` + `orchest-providers` (insulated at the `core::model` alias during migration).
- [ ] Both rulers fit with no provider-local content/event structs.
- [ ] Migration lands in independent green phases; no single big-bang PR.

## Dependencies

- v0.9.10 Minimax multimodal integration (shared multimodal `ContentBlock` groundwork).
- v0.9.11 omni realtime evidence (`docs/archive/iteration/v0_9_11/evidence.md`) for the realtime session shape.
- Existing ASR/TTS duplex streaming as lifecycle/test reference.
- `docs/todo/provider-unification.md` (the Step 3 mandate this PRD discharges).

## Verification

**Regression net.** Because early phases claim "no behavior change", the load-bearing guarantee is that the
**existing provider test suites stay green** through every phase — they are the characterization tests for
this refactor; no phase may delete a provider test without an equivalent replacement. Required local checks
for every phase:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

Weight-isolation evidence (recorded in the iteration note):

```bash
cargo tree -e features -p <consumer> --features llm   # must not contain tokio-tungstenite / oss signing
```

No live provider credentials are required for unit tests; dialect mapping and registry selection are
covered by fake sessions and descriptor fixtures.
