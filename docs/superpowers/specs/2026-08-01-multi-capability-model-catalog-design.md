# Multi-Capability Model Catalog Design

**Date:** 2026-08-01  
**Status:** Approved for planning  
**Implementation plan:** `docs/superpowers/plans/2026-08-01-multi-capability-model-catalog-batch-0-1.md` (Batch 0+1)

**Related:** ADR-0001 (provider unification), ADR-0002 (catalog as capability source), `orchest-protocol::CapabilityDescriptor`, `orchest-provider-http::catalog`

## Problem

Orchest has three inconsistent sources of “what models exist”:

1. **LLM catalog** (`orchest-provider-http::catalog`) — enumerable chat models with description, pricing, context window, thinking, modalities.
2. **Registry entries** (`Registry::with_builtin`) — Chat is expanded from the LLM catalog; Asr / Tts / Realtime / Gen mostly register **one default model per wire dialect**.
3. **Adapter passthrough** — many dialects accept arbitrary `cfg.model` strings, so unsupported or unlisted models can still be built, but cannot be discovered or described.

This is a historical gap, not an architectural conclusion:

- ADR-0001 requires static catalog data behind `CapabilityDescriptor` so the registry can filter **before** instantiation.
- ADR-0002 makes the catalog the canonical capability source for Chat.
- 2026-06-17 catalog hotfix explicitly deferred Asr / Tts / AIGC catalogs.

The user-facing symptom: “which streaming ASR models does provider `aliyun` support?” cannot be answered from code. Only the dialect default (`fun-asr-realtime` after the 2026-08-01 default change) is queryable.

## Goals

1. **Both discovery and runtime**, with **discovery first**.
2. **One catalog shape for all capabilities**, implemented in batches.
3. Every atomic model record has **at least one free-text description attribute** — including all existing Chat/LLM rows.
4. Preserve current build compatibility for uncataloged model IDs (passthrough), without pretending discovery is exhaustive.

## Non-Goals

- Live vendor model-list APIs / remote catalog sync.
- Hard allowlist that rejects every uncataloged model ID.
- Pricing completeness for non-Chat models in the first slices.
- Fully typed Asr/Tts/Realtime/Gen capability structs in slice 0 (typed fold may follow).
- Product UI / console.
- Changing wire protocols or adding new provider adapters just to fill catalog rows.

## Capability Boundaries

`Capability` remains the atomic product surface. One catalog row is one `(capability, provider, model)`.

| Capability | Meaning | Examples |
|---|---|---|
| **Chat** | Turn-based conversation (optionally multimodal input) | `anthropic/claude-opus-4-8`, `deepseek/deepseek-v4-flash` |
| **Asr** | Audio → text | `aliyun/fun-asr-realtime`, `aliyun/fun-asr-realtime-2026-02-28` |
| **Tts** | Text → audio | `aliyun/cosyvoice-v2` |
| **Realtime** | Duplex session / speech-to-speech / omni | Volcengine omni, Qwen omni realtime |
| **GenTask** | Async generation job | image / video / music gen |
| **VoiceManagement** | Voice enroll/design (out of first data batches) | — |

**Omni / speech-to-speech is Realtime, not Chat.**  
Multimodal Chat that *accepts* audio/image in a turn remains Chat. Continuous duplex audio session is Realtime.

**Dialect ≠ model.** One Aliyun ASR wire dialect can back many model rows. Registry factories stay dialect-scoped; catalog rows are model-scoped.

## Core Data Model

### `ModelRecord`

Canonical static row for any capability:

```rust
pub struct ModelRecord {
    /// Full catalog id: "provider/model".
    /// For Chat, `model` is the **resolved** model name after ADR-0002 parsing
    /// (protocol segments stripped). Nested vendor paths that are part of the
    /// model name (e.g. OpenRouter `anthropic/claude-…`) remain inside `model`.
    pub id: &'static str,
    pub provider: &'static str,
    pub model: &'static str,
    pub capability: Capability,

    pub display_name: &'static str,

    /// REQUIRED free-text description for humans / SDK discovery.
    /// Invariant: non-empty after trim. Applies to every capability, including Chat.
    /// Convention: Chinese technical one-liner (same as 2026-06-17 Chat hotfix),
    /// with a `// Source:` vendor-doc anchor on the data row.
    pub description: &'static str,

    pub input_modalities: &'static [Modality],
    pub output_modalities: &'static [Modality],

    pub streaming: bool,
    pub duplex: bool,
    pub interruptible: bool,
    pub tools: bool,
    pub thinking: bool,

    pub status: ModelStatus,
    /// At most one default per (capability, provider) **across all enabled
    /// features / impl crates** (see Invariants).
    pub default_for_provider: bool,

    pub pricing: Option<ModelPricing>,
    pub ext: CatalogExt,
}

pub enum ModelStatus {
    Stable,
    Snapshot,
    Preview,
    Deprecated,
}
```

**Lifetime lifetime:** public discovery returns `&'static ModelRecord`. Chat rows
until Batch 4 are not natively stored as `ModelRecord`; Batch 0 materializes the
aggregated catalog once into a process-global `LazyLock<Vec<ModelRecord>>` (or
equivalent) built by projecting `LlmModelEntry` + native non-Chat tables. That
materialization is a **single derived index**, not dual-write of two hand-edited
tables (see C3).

### Description invariant (hard)

- Every atomic model has `description: &'static str`.
- Empty / whitespace-only descriptions are rejected by unit tests / CI.
- Existing Chat `LlmModelEntry.description` maps 1:1 onto this field during migration.
- `display_name` is a short label; `description` is the human/technical free-text attr. They are not interchangeable.

### `CatalogExt`

Capability-specific detail. Progressive typing is allowed:

```rust
// Defined in orchest-provider-core (closed enum + all variant payload types).
pub enum CatalogExt {
    None,
    Chat(ChatCatalogExt),   // context_window, max_output_tokens, thinking detail, scenes, ...
    Asr(AsrCatalogExt),     // sample rates, formats, languages summary, diarization flags, ...
    Tts(TtsCatalogExt),
    Realtime(RealtimeCatalogExt),
    GenTask(GenCatalogExt),
}
```

**Type ownership (normative):** the closed `CatalogExt` enum and every payload
type it names (`ChatCatalogExt`, `AsrCatalogExt`, …) live in
**`orchest-provider-core`**. They are pure data (no reqwest/WS/OSS). Impl crates
only fill **tables of values**; they must not define competing ext enums.
Slice 0 may use minimal structs or `Value` placeholders inside those core types,
but **core `description` is never optional**.

### Projection

```text
ModelRecord
  ──CatalogEntry::descriptor()──► CapabilityDescriptor
                                  (core flags + CapabilityExt)
```

- Core flags (`streaming`, `duplex`, modalities, …) come from `ModelRecord`.
- Chat detail continues to ride in `CapabilityExt::Chat`.
- Non-Chat detail fills `CapabilityExt::Asr/Tts/Realtime/GenTask` as types mature.

## API Surface

Discovery is credential-free and network-free:

```rust
pub struct ModelFilter {
    pub capability: Option<Capability>,
    pub provider: Option<&'static str>,
    pub streaming: Option<bool>,
    pub duplex: Option<bool>,
    pub interruptible: Option<bool>,
    pub tools: Option<bool>,
    pub thinking: Option<bool>,
    pub status: Option<ModelStatus>,
    pub accepts: Option<Vec<Modality>>,
    pub emits: Option<Vec<Modality>>,
    /// When false (default for human discovery helpers), omit Deprecated.
    pub include_deprecated: bool,
}

fn list_models(filter: ModelFilter) -> impl Iterator<Item = &'static ModelRecord>;
fn find_model(id: &str) -> Option<&'static ModelRecord>;
fn find_model_for(id: &str, capability: Capability) -> Option<&'static ModelRecord>;
```

**ID parsing (ADR-0002, not naive first-slash):** catalog lookup **reuses the
ADR-0002 model-string resolver** (`normalize_provider_model` / equivalent), then
matches on the **resolved** `(provider, model)` — never on a raw “everything
after the first `/`” remainder when a protocol segment is present.

| Input | Resolved provider | Resolved model | Notes |
|---|---|---|---|
| `openai/gpt-5.4` | `openai` | `gpt-5.4` | plain |
| `openai/responses/gpt-5.4` | `openai` | `gpt-5.4` | protocol segment stripped |
| `elss/messages/claude-sonnet-5` | `elss` | `claude-sonnet-5` | protocol segment stripped |
| `elss/anthropic/claude-sonnet-5` | `elss` | `claude-sonnet-5` | provider-alias protocol |
| `openrouter/anthropic/claude-opus-4-8` | `openrouter` | `anthropic/claude-opus-4-8` | second segment is **not** a protocol → stays in model |
| `aliyun/fun-asr-realtime` | `aliyun` | `fun-asr-realtime` | non-Chat; no protocol vocabulary |

Catalog **row ids** store the canonical `provider/<resolved-model>` form (no
embedded protocol segment). Callers may pass protocol-explicit Chat ids into
`find_model*`; resolution peels the protocol before lookup.

**Protocol peeling is Chat-lookup-only (capability-aware).** The
`messages` / `chat` / `responses` (and provider-alias) vocabulary comes from
ADR-0002 and applies when resolving Chat model strings. Non-Chat catalog
lookup does not invent a protocol segment: it matches resolved
`provider/model` for that capability. Plan authors should not re-litigate a
second parser for Asr/Tts/Realtime/Gen.

Cross-capability collisions on the same resolved `provider/model` are handled
only by `find_model_for`. Bare ids (no `/`) remain ambiguous when multiple
capabilities share a bare name — Batch 0 may Chat-prefer for legacy
`find_model`; new code should prefer `find_model_for`.

**Discovery vs registry:** human/SDK model discovery (including `description`)
is the **catalog** API above. `Registry::*.list()` is for selection/build and
does **not** carry free-text description.

`ModelFilter` mirrors the registry’s capability flags that matter for discovery
(`streaming` / `duplex` / `interruptible` / `tools` / `thinking` / modalities /
status) so “streaming + tools” is expressible without opening the registry.

Example answers:

```rust
// Aliyun streaming ASR models
list_models(ModelFilter {
    capability: Some(Capability::Asr),
    provider: Some("aliyun"),
    streaming: Some(true),
    ..Default::default()
});

// Speech-to-speech / omni
list_models(ModelFilter {
    capability: Some(Capability::Realtime),
    duplex: Some(true),
    ..Default::default()
});
```

### Chat compatibility shims

| Legacy API | Behavior after unification |
|---|---|
| `list_models()` (LLM-only) | Shim to unified API with `capability = Chat`, or explicit `list_chat_models()` |
| `list_providers()` | Keep Chat provider aggregates first; multi-capability provider view is a follow-up |
| `find_model(id)` | Legacy Chat-first bare-id behavior allowed until Batch 4; new code uses `find_model_for` when capability is known |
| `LlmModelEntry` | Projection source until Batch 4; then reshape / deprecate |

### Dynamic providers

Chat already has non-enumerable gateways (`OpenRouter`, `Elss`). The unified
catalog must preserve a presence model:

```text
ProviderCatalogPresence =
  Known(rows) |
  Dynamic { description, model_id_format, model_id_example }
```

`list_models` enumerates only `Known` rows. Dynamic providers remain
discoverable via a provider-level API, not as fake static model rows.
Non-Chat capabilities need not ship Dynamic instances in early batches, but
the type must not assume “all providers are Known”.

## Registry Relationship

```text
catalog ModelRecord(s)
        │
        │ each enumerable row → descriptor + model-pinned factory
        ▼
Registry entries (per capability bucket)
        │
        ▼
Query::list / select / build
```

Rules:

1. **1 ModelRecord → 1 registry entry** for enumerable models.
2. **Shared dialect constructor, model-pinned entry factories:** rows that share
   a wire dialect reuse the same constructor function, but each registry
   `Entry` factory **pins** that row’s model id (same pattern as Chat
   `chat_entries`). `build(cfg)` must not silently substitute another catalog
   model.
3. **Default pick** uses `default_for_provider` (see Implementation Contracts).
4. **Registry.list()** for Asr/Tts/Realtime/Gen becomes catalog-expanded multi-row,
   not “one default per dialect”.
5. **Feature parity:** discovery and `with_builtin` expose the same
   feature-gated set (`http` / `stream` / `visual`). Catalog is not a global
   superset of disabled weight tiers.

### Uncataloged models (passthrough)

```text
find_model(id) == None
  → NOT added as a registry Entry
  → identity build still allowed via dialect free function / facade
    (e.g. asr::aliyun::from_provider_config(cfg))
  → if a descriptor is synthesized for telemetry, source = CapabilitySource::Assumed
  → discovery does not list it
  → validation stays conservative (do not invent capabilities)
```

Discovery is the **maintained** set, not a claim of vendor exhaustiveness.

## Implementation Contracts

These close the review gaps that would otherwise make Batch 1 ambiguous.
They are normative for planning and implementation.

### C1. Factory policy (cataloged vs uncataloged)

**Policy A — dual path (required):**

| Path | How it works |
|---|---|
| **Cataloged** | One `Entry` per `ModelRecord`. Factory closure pins that record’s model (mirrors `chat_entries`). Descriptor.model == pinned model. |
| **Uncataloged** | No registry `Entry`. Caller builds through dialect free functions / vendor facade with explicit `ProviderConfig.model`. |

Rejected alternatives:

- Single shared factory that always trusts `cfg.model` while advertising multiple catalog descriptors (select/build mismatch).
- Catch-all registry entries for “any model on this dialect” (pollutes `select()`).
- Hard-reject all uncataloged ids (contradicts Non-Goals).

Dialect constructor code is still shared (one Aliyun inference-WS implementation);
**pinning happens at Entry construction**, not by forking wire code per model.

### C2. `Query::select` / default policy

Today `Query::select` returns the first match after `(provider, model)` sort.
After multi-model expansion that is **not** an acceptable silent default.

Required selection rules when multiple entries match the query filters:

1. **Exactly one match** → select it.
2. **Multiple matches, exactly one with `default_for_provider = true`** → select that default.
3. **Multiple matches, zero or >1 defaults** → error (`NoMatchingProvider` or a dedicated ambiguous code). Caller must narrow with `.id("provider/model")` (or equivalent).
4. **`(provider, model)` sort** remains for **`list()` ordering only**, never as a silent multi-match winner.

Deprecated status:

- Discovery helpers omit `Deprecated` unless `include_deprecated = true`.
- `select()` should not prefer Deprecated over a Stable/Snapshot default; if the only matches are Deprecated, selecting them is allowed but should be explicit in tests.

Implementations may encode default preference by sorting key or by post-filter; observable behavior must match the rules above.

#### C2a. Existing multi-match select surfaces (breakage — must plan)

C2 is a **registry-wide** behavior change as soon as default-aware `select`
ships (Batch 1 touches `Query`, not only Asr data). Blast radius is larger than
“multi-Entry Gen providers”:

**A. Multi-entry same provider (silent sort today)**

| Capability | Provider | Entries today | Silent `.provider(p).select()` winner (sort) |
|---|---|---|---|
| GenTask | `volcengine` | image (`doubao-seedream-…`) + video (`doubao-seedance-…`) | first by model string sort |
| GenTask | `aliyun` | music (`fun-music-v1`, http) + image (`wanx2.1-t2i-turbo`, visual) | first by model string sort |

**B. Capability-only / cross-provider select (no provider, no id)**

Any query that matches rows from **multiple providers** with **zero**
`default_for_provider` hits C2 rule 3 and **errors**. That is **intentional**:
there is no unique product default across vendors. Examples already in-tree:

| Location | Query | Today | After C2 |
|---|---|---|---|
| `crates/orchest-provider/tests/selection.rs` (~L112) | `.chat().accepts([Image]).thinking().select()` | sort → `openai` | **Err** (anthropic/openai/doubao multi-hit, no defaults) |
| `crates/orchest-provider/src/lib.rs` doctest | `.chat().accepts([Text, Image]).thinking().select()` | `let _ =` hides Result; runtime would be Ok via sort | runtime **Err**; docs become misleading if left as “happy path” |

**C. Chat per-provider multi-row, no defaults yet**

Anthropic / OpenAI / DeepSeek / Volcengine each register many Chat rows and
**no** `default_for_provider` until Batch 4 (or an explicit earlier decision).
Therefore `.provider("anthropic").chat().select()` also becomes an error under
C2 unless a Chat default is declared. No in-repo production callsite is known
today; this is still a **public API contract** change and must be documented,
not “fixed” by reintroducing silent sort.

**Known production-ish callsites (provider-only select/build):**

- `examples/demo/music-gift/src/config.rs` — `.gen().provider(provider).build(...)` (provider from user config; music path expects music, not wanx)
- `examples/demo/briefing-desk/src/media.rs` — `.asr()` / `.tts().provider(provider).build(...)` (safe today while Asr/Tts are 1:1; becomes load-bearing once multi-model)

**Required before/with C2 landing (plan checklist, not optional):**

1. **Callsite audit** of `.select()` / `.build()` across workspace (examples, demos, tests, bindings, doctests) — include **capability-only** queries, not only `.provider(...)`.
2. **Declare product defaults** for every multi-entry `(capability, provider)` that must keep provider-only select working, **or** change callsites to `.id("provider/model")` / `.list()` then choose.
3. **Minimum default declarations for current Gen multi-entry providers** (unless audit rewrites all callsites first):

   | (capability, provider) | `default_for_provider` | Rationale |
   |---|---|---|
   | `(GenTask, volcengine)` | image seedream row | safer generic default than video; video callers should `.id` |
   | `(GenTask, aliyun)` | `fun-music-v1` (http music) | music-gift uses provider-only build; wanx remains `.id` / explicit |

   If product disagrees, flip the default table in the plan — but **some** explicit choice is mandatory; silent sort is not.

4. **Do not declare Chat per-provider defaults before Batch 4** unless a separate product decision lands earlier. Until then, Chat multi-row provider-only or capability-only `select` is expected to error; callers use `.id` / `.list()`.
5. **Update in-tree tests/docs as contract adoption, not regressions:**
   - `selection.rs` capability-only multimodal+thinking test: either assert `Err`, or add `.provider(...)` / `.id(...)` if the test still wants a concrete pick.
   - `lib.rs` doctest: stop presenting capability-only `select()` as a success path; show `.list()` then choose, or provider/id narrowing.
6. Tests under **`--all-features`** proving declared Gen defaults win and that dual-default across crates fails CI (invariant 4). Plan must add this step explicitly; default `cargo test --workspace` alone is not enough (see Invariants).

**Do not weaken C2** to keep the old silent cross-provider winner. The honest
contract is: multi-match without a unique default → error.

### C3. Batch 0 Chat integration = projection + one-time materialization, not dual-write

Until Batch 4:

- Unified discovery builds a process-global materialized index
  (`LazyLock<Vec<ModelRecord>>` or equivalent) that **projects** existing
  `LlmModelEntry` rows into `ModelRecord` values and appends native non-Chat
  tables.
- **Forbidden:** two hand-maintained Chat tables (edit Chat data in one place only:
  today’s `LlmModelEntry` catalog until Batch 4).
- Chat `description` comes from the existing field; the non-empty invariant
  applies to the projected records immediately.
- Batch 4 replaces projection with native `ModelRecord` storage and leaves
  legacy APIs as thin shims.

### C4. Description lives on catalog, not descriptor

- `ModelRecord.description` is the required free-text attribute.
- `CapabilityDescriptor` stays flag/modality oriented for registry query; it
  does **not** gain a description field in this design.
- Success criterion “can discover models and their descriptions” means
  **catalog** `list_models` / `find_model*`, not `Registry::list()`.
- **Language / provenance:** follow the Chat hotfix convention — Chinese
  technical one-liner + `// Source:` vendor-doc anchor on each data row — for
  all capabilities so style does not drift.

### C5. Descriptor source for passthrough

When an uncataloged build synthesizes a descriptor, set
`CapabilitySource::Assumed`. Do not invent a new source variant unless a later
ADR requires it. Cataloged rows use `CapabilitySource::Static`.

### C6. Crate layering (types vs tables)

| Layer | Holds |
|---|---|
| `orchest-provider-core` | **`ModelRecord`, `ModelFilter`, `ModelStatus`, closed `CatalogExt` enum, and all `*CatalogExt` payload types** (pure data). Materialization helpers may live here or in the wall. |
| `orchest-provider-http` | **Chat data tables** (+ projection into materialized index until Batch 4). Does **not** own `CatalogExt` / `ChatCatalogExt` type definitions. |
| `orchest-provider-stream` / `-visual` | Asr / Tts / Realtime / Gen **data tables** constructing core ext values. |
| `orchest-provider` | Aggregated discovery API + registry expansion from cataloged rows; C2 select behavior. |

Rationale: stream/visual must construct `CatalogExt::Asr` / `::GenTask` without
depending on http; http must construct `CatalogExt::Chat` without depending on
stream. Therefore the closed enum cannot live in a tier crate.

Core must not take websocket/OSS deps. Pure-LLM consumers remain light.

## Crate Placement

| Layer | Responsibility |
|---|---|
| `orchest-protocol` | `Capability`, `CapabilityDescriptor`, `CatalogEntry`, modalities, `CapabilitySource` (already) |
| `orchest-provider-core` | `ModelRecord`, `ModelFilter`, `ModelStatus`, `CatalogExt` skeleton (no heavy deps) |
| Impl crates (`-http` / `-stream` / `-visual`) | Static tables / projections for their dialects |
| `orchest-provider` | Aggregate `list_models` / `find_model*`; `Registry::with_builtin` expands from catalog |

Chat data currently in `orchest-provider-http/src/catalog` is **projected** into
the unified index from Batch 0, then **reshaped** onto native `ModelRecord`
storage in Batch 4 (see C3).

## Rollout Batches

### Batch 0 — Skeleton

- Introduce core types: `ModelRecord`, `ModelFilter` (incl. `tools`/`thinking`),
  `ModelStatus`, closed `CatalogExt` + payload stubs in **provider-core**.
- Materialized `LazyLock` index; **project** Chat `LlmModelEntry` (C3); no dual-write.
- Unified `list_models` / `find_model(_for)` using **ADR-0002 resolution** for Chat ids.
- Description non-empty invariant tests (Chat projection included).
- Uniqueness tests on `(capability, provider, model)`.
- Dynamic provider presence type reserved (may only have Chat instances).
- No consumer breakage: Chat legacy APIs remain.

### Batch 1 — Asr data + registry expansion + C2 select

Minimum Aliyun streaming rows:

| id | notes |
|---|---|
| `aliyun/fun-asr-realtime` | default (`default_for_provider = true`) |
| `aliyun/fun-asr-realtime-2026-02-28` | Fun-ASR realtime snapshot on same inference-WS dialect (Batch 1 multi-model proof; Qwen realtime needs a separate dialect later) |

Also register at least the current default row for other already-wired ASR dialects (volcengine, deepgram, soniox, elevenlabs, assemblyai, speechmatics) so every **Asr** dialect has ≥1 catalog row with description.

Registry Asr bucket expands from catalog with **model-pinned** factories (C1).

**Ship C2 select rules in this batch** (registry code), together with C2a:

- Callsite audit + declare Gen defaults for `volcengine` / `aliyun` (or rewrite demos to `.id`).
- Selection tests:
  - Asr `list` returns multiple Aliyun models;
  - `.provider("aliyun").asr().select()` returns `fun-asr-realtime`;
  - `.id("aliyun/fun-asr-realtime-2026-02-28")` pins the non-default;
  - multi-match without default errors (fixture);
  - Gen provider-only select still works for declared defaults under `--all-features`.

### Batch 2 — Realtime (incl. S2S / omni)

- Catalog Realtime rows. **Identity note:** today’s registry pins Volcengine omni
  as model `"1.2.1.1"` (openspeech protocol/resource version, not a marketing
  model id), and tests assert that string. Batch 2 must either:
  - keep `"1.2.1.1"` as the catalog `model` / default with a clear description
    that this is the openspeech omni dialect version, **or**
  - introduce a real product model id, teach `from_provider_config` to accept
    `cfg.model`, and update selection tests in the same change.
- Explicit documentation/tests: Realtime ≠ Chat.
- Discovery query for duplex speech-to-speech works without reading docs.
  Invariant 3 for Realtime dialects applies from this batch.


### Batch 3 — Tts + GenTask

- Expand beyond dialect defaults.
- Descriptions required; pricing optional.

### Batch 4 — Chat migration

- Move `LlmModelEntry` fields onto `ModelRecord { capability: Chat, ext: Chat(...) }`.
- Preserve every Chat `description`.
- Legacy Chat APIs become shims.
- ADR-0002 “catalog is capability source” continues to hold under the unified type.

## Invariants / CI

1. `description.trim().is_empty() == false` for every catalog record (including Chat projection). **Global from Batch 0** for whatever rows exist.
2. `(capability, provider, model)` unique across the aggregated catalog. **Global from Batch 0**; run under default features **and** `--all-features` (cross-crate collisions only show up with all tiers linked — e.g. aliyun Gen music+wanx).
3. Every registered dialect has ≥1 catalog row. **Phased:** enforced per capability as that capability’s data batch lands (Asr: Batch 1; Realtime: Batch 2; Tts/Gen: Batch 3). Do not fail CI for Tts/Gen empty catalogs before Batch 3.
4. At most one `default_for_provider` per `(capability, provider)` **across all features**. **Global uniqueness check under `--all-features` from Batch 1** (when C2 ships); feature-gated unit tests alone are insufficient. **CI must add an explicit all-features job/step** for this check — it is outside the default `CONVENTIONS.md` trio (`cargo test --workspace`, clippy, fmt) which does not enable all provider features.
5. `CatalogEntry::descriptor()` flags match record core flags; cataloged source = `Static`.
6. Chat migration / projection: every prior Chat description remains non-empty and mapped.
7. Cataloged entry factories pin their model (C1); uncataloged builds do not create Entries.
8. `select()` multi-match behavior matches C2 (default preference, no silent dict-order win) from Batch 1 onward — including intentional errors for capability-only multi-provider matches and Chat multi-row providers without defaults (C2a).

## Success Criteria

- **Catalog** API can answer: “Aliyun streaming ASR models?” (with descriptions) without vendor docs.
- **Catalog** API can answer: “which Realtime/S2S models exist?” without conflating them with Chat.
- Every cataloged model, including all Chat models, exposes a free-text `description` (Chinese + source anchor convention).
- Registry Asr (then other capabilities) lists catalog-expanded models, not only dialect defaults.
- `.provider(p).select()` returns the catalog default when multiple models exist for `p`; multi-entry Gen providers keep working via declared defaults or updated `.id` callsites (C2a). Capability-only multi-provider `select` errors by design; `selection.rs` / `lib.rs` doctest updated accordingly.
- `find_model("openai/responses/gpt-5.4")` resolves via ADR-0002 and hits the Chat row for `gpt-5.4`.
- Uncataloged model IDs still build via dialect free functions / facade; they do not appear in discovery and do not create registry entries.
- No pure-LLM consumer is forced to take websocket/OSS deps (weight isolation preserved by crate placement).

## Decisions Log

| Decision | Choice | Rationale |
|---|---|---|
| Primary goal | Discovery + runtime, discovery first | User need is queryability; runtime source-of-truth follows same data |
| Scope | All capabilities designed once; batched implementation | Avoid parallel catalog shapes |
| Architecture | Unified `ModelRecord` + `CatalogExt` | Aligns with ADR-0001/0002; Chat migrates into same shape |
| Description | Required free-text on every atomic model; catalog-only; zh + `// Source:` | Explicit product requirement; keep descriptor query-thin; match Chat hotfix |
| Omni classification | `Capability::Realtime` | Duplex S2S is not turn-based Chat |
| ID parsing | ADR-0002 resolver (protocol peel), Chat-lookup-only | Protocol-explicit Chat ids must find rows; non-Chat has no protocol vocab |
| Uncataloged models | Free-function / facade passthrough; no Entry; `Assumed` | Compatibility without false completeness |
| Factory policy | Model-pinned Entries + shared dialect constructor (C1) | Matches Chat `chat_entries`; avoids select/build skew |
| Select policy | Default unique → pick; else error (C2) | Dict-order is not a product default |
| Multi-match blast radius | Gen defaults + capability-only intentional Err; Chat no defaults until Batch 4; update selection/doctest (C2a) | Cross-provider silent wins are not unique solutions |
| Batch 0 Chat | Projection + LazyLock materialization, no dual-write (C3) | `'static` discovery without table fork |
| CatalogExt types | All in provider-core; impl crates hold tables only (C6) | Closed enum must be reachable from every tier crate |
| First data | Asr (Aliyun fun-asr multi-snapshot on inference WS), then Realtime | Matches current product questions; Qwen ASR realtime is a later dialect |

## Open Follow-ups (not blocking this design)

- Exact minimal fields for `AsrCatalogExt` / `RealtimeCatalogExt` (sample rates, languages, endpointing).
- Whether provider-level aggregate views (`list_providers` multi-capability) ship with Batch 0 or later.
- Whether legacy bare `find_model` stays Chat-preferring after Batch 4 or becomes error-on-ambiguity.
- When non-Chat pricing becomes required rather than optional.
- Whether multi-match select uses a new `AmbiguousProvider` error code or reuses `NoMatchingProvider` with a clear message.
- Whether Realtime omni keeps catalog model `"1.2.1.1"` long-term or renames to a product id (Batch 2 decision).
- Whether this design should be mirrored/linked from `docs/iteration/roadmap.md` (currently lives under `docs/superpowers/specs/`).
- ADR-0001 action checklist items 002–008 are historically completed relative to the live registry; treat that ADR as architectural rationale, not an open todo list.

## Implementation Next Step

After user review of this spec, create an implementation plan (writing-plans) starting at **Batch 0 + Batch 1**, implementing **Implementation Contracts C1–C6 and C2a (callsite audit + Gen defaults + capability-only test/doctest updates)**, with Chat kept on projection/shims until Batch 4.
