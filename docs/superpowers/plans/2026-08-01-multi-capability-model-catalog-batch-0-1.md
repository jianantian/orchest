# Multi-Capability Model Catalog (Batch 0 + Batch 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a unified multi-capability model catalog skeleton (discovery + Chat projection) and expand Asr registry rows with default-aware `select()` (C1–C6, C2a), without migrating Chat storage off `LlmModelEntry`.

**Architecture:** Pure catalog types live in `orchest-provider-core`. Impl crates own static tables. The wall (`orchest-provider`) materializes a process-global `LazyLock` index for discovery and expands registry entries from catalog rows. `Query::select` stops silent dict-order wins: unique match or unique `default_for_provider`, else error. Cataloged factories pin model ids at Entry construction (Chat pattern); uncataloged models stay free-function passthrough.

**Tech Stack:** Rust workspace crates `orchest-protocol`, `orchest-provider-core`, `orchest-provider-http`, `orchest-provider-stream`, `orchest-provider-visual`, `orchest-provider`; `std::sync::LazyLock`; existing `CapabilityDescriptor` / `Entry` / `Query` surfaces.

**Spec:** `docs/superpowers/specs/2026-08-01-multi-capability-model-catalog-design.md`

## Global Constraints

- Discovery first, but Batch 1 also lands registry/runtime contracts C1–C2/C2a.
- Every catalog `ModelRecord.description` is non-empty after trim (Chinese one-liner + `// Source:` on data rows).
- No dual-write of Chat tables: project `LlmModelEntry` until Batch 4 (C3).
- `description` is catalog-only — do **not** add it to `CapabilityDescriptor` (C4).
- Cataloged rows → `CapabilitySource::Static`; uncataloged synthesized descriptors → `Assumed` (C5).
- `CatalogExt` + all payload types live in **provider-core**; http/stream/visual only hold tables (C6).
- No Chat per-provider defaults before Batch 4.
- C2 multi-match without unique default is **intentional error** — do not reintroduce silent sort.
- Protocol peeling for Chat id lookup reuses ADR-0002 (`normalize_provider_model`); non-Chat does not invent protocol segments.
- Pure-LLM consumers must not gain websocket/OSS deps.
- Skip formatters mid-task; run targeted tests per task. Final task runs workspace checks + all-features selection tests.

---

## File Structure

| Path | Responsibility |
|---|---|
| `crates/orchest-protocol/src/descriptor.rs` | Add `default_for_provider: bool` to `CapabilityDescriptor` (+ builder). Selection needs this flag on entries. |
| `crates/orchest-provider-core/src/catalog.rs` | **Create.** `ModelRecord`, `ModelStatus`, `ModelFilter`, `CatalogExt` + payload stubs, `ProviderCatalogPresence`, projection helpers, `CatalogEntry` for `ModelRecord`. |
| `crates/orchest-provider-core/src/lib.rs` | `pub mod catalog;` re-exports. |
| `crates/orchest-provider/src/catalog.rs` | **Create.** Materialized index, `list_models` / `find_model` / `find_model_for`, Chat projection under `http`, Asr table aggregation under features. |
| `crates/orchest-provider/src/lib.rs` | Re-export discovery API; fix doctest for C2. |
| `crates/orchest-provider/src/registry.rs` | C2 `Query::select` default-aware rules. |
| `crates/orchest-provider-stream/src/catalog/mod.rs` (+ `asr.rs`) | **Create.** Asr static tables (aliyun multi-model + other stream ASR defaults). |
| `crates/orchest-provider-stream/src/lib.rs` | Expand `asr_entries()` from catalog; model-pinned factories. |
| `crates/orchest-provider-stream/src/asr/aliyun.rs` | Keep free-function passthrough; pin helpers used by entries. |
| `crates/orchest-provider-http/src/catalog/asr.rs` (or stream-parallel path) | AssemblyAI + Speechmatics default Asr rows (http weight). |
| `crates/orchest-provider-http/src/lib.rs` | Expand `asr_entries()` from catalog; pin factories. |
| `crates/orchest-provider-http/src/gen/aliyun_music.rs` | Mark `default_for_provider = true` on music entry descriptor (C2a). |
| `crates/orchest-provider-visual/src/gen/volcengine.rs` | Mark image seedream `default_for_provider = true` (C2a). |
| `crates/orchest-provider-visual/src/gen/volcengine_video.rs` | Explicit non-default (leave false). |
| `crates/orchest-provider-visual/src/gen/aliyun.rs` | Explicit non-default (wanx). |
| `crates/orchest-provider/tests/selection.rs` | C2 fixture tests; Aliyun multi-model; Gen defaults under all-features. |
| `crates/orchest-provider/tests/catalog_discovery.rs` | **Create.** Description/uniqueness/list/find tests. |
| `.github/workflows/ci.yml` | Add all-features selection/catalog job step for invariant 4. |

**Out of this plan (later batches):** Realtime/omni catalog (Batch 2), full Tts/Gen multi-model catalogs (Batch 3), native Chat `ModelRecord` storage (Batch 4).

---

### Task 1: Core catalog types + descriptor default flag

**Files:**
- Create: `crates/orchest-provider-core/src/catalog.rs`
- Modify: `crates/orchest-provider-core/src/lib.rs`
- Modify: `crates/orchest-protocol/src/descriptor.rs`
- Test: unit tests inside `catalog.rs` (or `catalog/tests` submodule)

**Interfaces:**
- Produces:
  - `ModelStatus { Stable, Snapshot, Preview, Deprecated }`
  - `ModelRecord { id, provider, model, capability, display_name, description, input_modalities, output_modalities, streaming, duplex, interruptible, tools, thinking, status, default_for_provider, pricing, ext }`
  - `CatalogExt { None, Chat(ChatCatalogExt), Asr(AsrCatalogExt), Tts(TtsCatalogExt), Realtime(RealtimeCatalogExt), GenTask(GenCatalogExt) }`
  - Minimal payload stubs (Batch 0 may be empty structs or only Chat fields used by projection):
    - `ChatCatalogExt { context_window: u64, max_output_tokens: Option<u32>, max_input_tokens: Option<u64>, thinking_max_tokens: Option<u32> }`
    - `AsrCatalogExt {}` (empty for now)
    - others empty
  - `ModelFilter` with `Default` (all `None` / false `include_deprecated`)
  - `ModelRecord::to_descriptor(&self) -> CapabilityDescriptor`
  - `impl CatalogEntry for ModelRecord`
  - `CapabilityDescriptor::default_for_provider: bool` + `.default_for_provider(bool)` builder (default `false`)

- [ ] **Step 1: Write failing core tests for description invariant helper**

Add at bottom of new `catalog.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::{Capability, Modality};

    fn sample(description: &'static str) -> ModelRecord {
        ModelRecord {
            id: "aliyun/fun-asr-realtime",
            provider: "aliyun",
            model: "fun-asr-realtime",
            capability: Capability::Asr,
            display_name: "Fun-ASR Realtime",
            description,
            input_modalities: &[Modality::Audio],
            output_modalities: &[Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        }
    }

    #[test]
    fn description_must_be_non_empty() {
        assert!(sample("streaming ASR").description_is_valid());
        assert!(!sample("").description_is_valid());
        assert!(!sample("   ").description_is_valid());
    }

    #[test]
    fn to_descriptor_copies_default_flag_and_source_static() {
        let d = sample("ok").to_descriptor();
        assert!(d.default_for_provider);
        assert_eq!(d.source, orchest_protocol::CapabilitySource::Static);
        assert_eq!(d.model.as_ref(), "fun-asr-realtime");
        assert!(d.streaming && d.duplex);
    }
}
```

- [ ] **Step 2: Run test to verify it fails (module missing)**

Run: `cargo test -p orchest-provider-core description_must_be_non_empty -- --nocapture`  
Expected: compile error / module not found.

- [ ] **Step 3: Implement types**

In `crates/orchest-protocol/src/descriptor.rs`, add field + builder:

```rust
// on CapabilityDescriptor
pub default_for_provider: bool,

// in new():
default_for_provider: false,

// builder:
#[must_use]
pub fn default_for_provider(mut self, v: bool) -> Self {
    self.default_for_provider = v;
    self
}
```

Update any exhaustive struct literals / tests that construct `CapabilityDescriptor` manually if they break.

In `crates/orchest-provider-core/src/catalog.rs`, implement the types from the design (all `'static` where applicable). Include:

```rust
impl ModelRecord {
    pub fn description_is_valid(&self) -> bool {
        !self.description.trim().is_empty()
    }

    pub fn to_descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new(self.provider, self.model, self.capability)
            .with_input_modalities(self.input_modalities.iter().copied())
            .with_output_modalities(self.output_modalities.iter().copied())
            .streaming(self.streaming)
            .tools(self.tools)
            .thinking(self.thinking)
            .duplex(self.duplex)
            .interruptible(self.interruptible)
            .default_for_provider(self.default_for_provider)
            .with_source(CapabilitySource::Static)
            // Chat ext projection can be filled when capability is Chat; else leave None for Batch 0 non-Chat
    }
}

impl CatalogEntry for ModelRecord {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.to_descriptor()
    }
}
```

Wire `pub mod catalog;` and re-export key types from `orchest-provider-core/src/lib.rs`.

Note: `CapabilityDescriptor::with_input_modalities` today takes `IntoIterator`/`[Modality]` — match existing signature; adapt if needed.

- [ ] **Step 4: Run tests**

Run: `cargo test -p orchest-provider-core catalog:: -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/orchest-protocol/src/descriptor.rs \
  crates/orchest-provider-core/src/catalog.rs \
  crates/orchest-provider-core/src/lib.rs
git commit -m "feat: add ModelRecord catalog types and default_for_provider flag"
```

---

### Task 2: Wall discovery materialization + Chat projection (C3)

**Files:**
- Create: `crates/orchest-provider/src/catalog.rs`
- Create: `crates/orchest-provider/tests/catalog_discovery.rs`
- Modify: `crates/orchest-provider/src/lib.rs`
- Modify: `crates/orchest-provider/Cargo.toml` (dev-deps only if needed)

**Interfaces:**
- Consumes: `ModelRecord`, `ModelFilter`, `LlmModelEntry` (feature `http`)
- Produces:
  - `pub fn list_models(filter: ModelFilter) -> impl Iterator<Item = &'static ModelRecord>`
  - `pub fn find_model(id: &str) -> Option<&'static ModelRecord>`
  - `pub fn find_model_for(id: &str, capability: Capability) -> Option<&'static ModelRecord>`
  - Internal: `static CATALOG: LazyLock<Vec<ModelRecord>>`
  - `fn project_llm(entry: &LlmModelEntry) -> ModelRecord`

**ID resolution rules (implement exactly):**
1. For `find_model` / `find_model_for(..., Chat)` when feature `http` is on: call `orchest_provider_http::normalize_provider_model(id)`. On `Ok`, match rows where `provider == n.provider && model == n.model && capability == Chat` (or any capability for bare `find_model`). On `Err` (unknown provider etc.), fall back to non-Chat simple split.
2. Non-Chat / fallback: `provider/model` via first `/` only when no Chat protocol peel applies; bare id matches `model` field; multi-capability bare id Chat-prefers for `find_model`.
3. Protocol peel is **Chat-lookup-only** — never invent protocol segments for Asr/Tts/Realtime/Gen.

- [ ] **Step 1: Write failing discovery tests**

`crates/orchest-provider/tests/catalog_discovery.rs`:

```rust
#![cfg(feature = "http")]

use orchest_protocol::Capability;
use orchest_provider::catalog::{find_model, find_model_for, list_models, ModelFilter};

#[test]
fn projected_chat_rows_have_non_empty_descriptions() {
    let rows: Vec<_> = list_models(ModelFilter {
        capability: Some(Capability::Chat),
        include_deprecated: true,
        ..Default::default()
    })
    .collect();
    assert!(!rows.is_empty());
    for r in rows {
        assert!(
            !r.description.trim().is_empty(),
            "empty description for {}",
            r.id
        );
    }
}

#[test]
fn chat_ids_are_unique_by_capability_provider_model() {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for r in list_models(ModelFilter {
        include_deprecated: true,
        ..Default::default()
    }) {
        assert!(
            seen.insert((r.capability, r.provider, r.model)),
            "duplicate {:?}",
            (r.capability, r.provider, r.model)
        );
    }
}

#[test]
fn find_model_peels_openai_responses_protocol() {
    let hit = find_model("openai/responses/gpt-5.4")
        .expect("protocol-explicit id should resolve via ADR-0002");
    assert_eq!(hit.provider, "openai");
    assert_eq!(hit.model, "gpt-5.4");
    assert_eq!(hit.capability, Capability::Chat);
}

#[test]
fn find_model_for_disambiguates_capability() {
    // After Batch 1 Asr rows land this matters more; for Batch 0 assert Chat path.
    let hit = find_model_for("openai/gpt-5.4", Capability::Chat).expect("chat row");
    assert_eq!(hit.capability, Capability::Chat);
}
```

- [ ] **Step 2: Run tests — expect fail**

Run: `cargo test -p orchest-provider --features http --test catalog_discovery -- --nocapture`  
Expected: compile fail (module missing).

- [ ] **Step 3: Implement wall catalog module**

Skeleton:

```rust
// crates/orchest-provider/src/catalog.rs
use std::sync::LazyLock;
use orchest_protocol::{Capability, Modality as ProtoModality};
use orchest_provider_core::catalog::{
    CatalogExt, ChatCatalogExt, ModelFilter, ModelRecord, ModelStatus,
};

static CATALOG: LazyLock<Vec<ModelRecord>> = LazyLock::new(build_catalog);

fn build_catalog() -> Vec<ModelRecord> {
    let mut out = Vec::new();
    #[cfg(feature = "http")]
    {
        for m in orchest_provider_http::catalog::list_models() {
            out.push(project_llm(m));
        }
    }
    // Batch 1 appends Asr tables here under stream/http features.
    out
}

#[cfg(feature = "http")]
fn project_llm(m: &orchest_provider_http::catalog::LlmModelEntry) -> ModelRecord {
    let model = m
        .model_id
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(m.model_id);
    ModelRecord {
        id: m.model_id,
        provider: m.provider,
        model,
        capability: Capability::Chat,
        display_name: m.display_name,
        description: m.description,
        input_modalities: map_modalities_static(m.input_modalities), // see note
        // ...
        streaming: true,
        duplex: false,
        interruptible: false,
        tools: true,
        thinking: m.thinking.is_some(),
        status: ModelStatus::Stable,
        default_for_provider: false, // no Chat defaults until Batch 4
        pricing: m.pricing.clone(),
        ext: CatalogExt::Chat(ChatCatalogExt {
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            max_input_tokens: m.max_input_tokens,
            thinking_max_tokens: m.thinking.and_then(|t| t.max_thinking_tokens),
        }),
    }
}
```

**Modality mapping note:** `LlmModelEntry` uses catalog-local `Modality` in http crate; spine uses `orchest_protocol::Modality`. Prefer storing spine modalities on `ModelRecord` (`&'static [ProtoModality]`). Projection can map via a small helper; if `'static` mapping of dynamic slices is awkward, store owned `Vec<Modality>` on `ModelRecord` for projected rows only — but design wants `'static`. Practical Batch 0 approach: change `ModelRecord` modality fields to `Vec<Modality>` **or** map known slices through `to_proto_modality` into leaked/`once_cell` statics. Prefer **`Vec<Modality>` on `ModelRecord`** if it simplifies projection without dual modality enums in the public discovery type — update Task 1 type if still open; if Task 1 already shipped `&'static [Modality]`, leak mapped vecs only in tests or use spine modalities in projection tables only.

Recommended decision for implementers: **`ModelRecord.input_modalities` / `output_modalities` are `Vec<Modality>` (spine)** for projection ease. Update Task 1 if needed in same PR before this task lands.

Public API:

```rust
pub fn list_models(filter: ModelFilter) -> impl Iterator<Item = &'static ModelRecord> {
    CATALOG.iter().filter(move |r| filter.matches(r))
}

impl ModelFilter {
    pub fn matches(&self, r: &ModelRecord) -> bool { /* capability/provider/flags/modalities/status/include_deprecated */ }
}
```

Re-export from wall `lib.rs`:

```rust
pub mod catalog;
pub use catalog::{find_model, find_model_for, list_models, ModelFilter};
pub use orchest_provider_core::catalog::{CatalogExt, ModelRecord, ModelStatus};
```

Ensure http catalog module is reachable: `orchest_provider_http::catalog` is already public.

- [ ] **Step 4: Run discovery tests**

Run: `cargo test -p orchest-provider --features http --test catalog_discovery -- --nocapture`  
Expected: PASS (Chat projection only).

Also: `cargo test -p orchest-provider-http catalog:: -- --nocapture`  
Expected: existing Chat catalog tests still PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/orchest-provider/src/catalog.rs \
  crates/orchest-provider/src/lib.rs \
  crates/orchest-provider/tests/catalog_discovery.rs \
  crates/orchest-provider-core/src/catalog.rs
git commit -m "feat: materialize multi-capability catalog with Chat projection"
```

---

### Task 3: C2 default-aware `Query::select` + fixture tests (C2a B)

**Files:**
- Modify: `crates/orchest-provider/src/registry.rs`
- Modify: `crates/orchest-provider/tests/selection.rs`
- Modify: `crates/orchest-provider/src/lib.rs` (doctest)

**Interfaces:**
- Consumes: `CapabilityDescriptor.default_for_provider`
- Produces: `Query::select` behavior per C2 rules 1–4
- Error: reuse `ErrorCode::NoMatchingProvider` with message containing `ambiguous` when multi-match without unique default (document in message; dedicated code deferred)

- [ ] **Step 1: Write failing fixture tests for multi-match policy**

In `selection.rs`, extend fixture or add a local registry:

```rust
#[test]
fn select_errors_when_multiple_match_without_default() {
    let mut reg = Registry::new();
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime", Capability::Asr)
            .streaming(true)
            .duplex(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "fun-asr-realtime")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime-2026-02-28", Capability::Asr)
            .streaming(true)
            .duplex(true),
        |_| {
            Ok(Box::new(FakeAsr(
                "aliyun",
                "fun-asr-realtime-2026-02-28",
            )) as Box<dyn Asr>)
        },
    ));
    let err = reg
        .asr()
        .provider("aliyun")
        .select()
        .expect_err("multi-match without default must error under C2");
    assert_eq!(err.code, orchest_protocol::ErrorCode::NoMatchingProvider);
    assert!(
        err.message.to_lowercase().contains("ambiguous")
            || err.message.to_lowercase().contains("multiple"),
        "message should explain ambiguity: {}",
        err.message
    );
}

#[test]
fn select_prefers_unique_default_for_provider() {
    let mut reg = Registry::new();
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime", Capability::Asr)
            .streaming(true)
            .duplex(true)
            .default_for_provider(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "fun-asr-realtime")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime-2026-02-28", Capability::Asr)
            .streaming(true)
            .duplex(true),
        |_| {
            Ok(Box::new(FakeAsr(
                "aliyun",
                "fun-asr-realtime-2026-02-28",
            )) as Box<dyn Asr>)
        },
    ));
    let picked = reg.asr().provider("aliyun").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "fun-asr-realtime");
}

#[test]
fn list_still_sorts_by_provider_model_without_default_bias() {
    let mut reg = Registry::new();
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime", Capability::Asr)
            .default_for_provider(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "fun-asr-realtime")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime-2026-02-28", Capability::Asr),
        |_| {
            Ok(Box::new(FakeAsr(
                "aliyun",
                "fun-asr-realtime-2026-02-28",
            )) as Box<dyn Asr>)
        },
    ));
    let list = reg.asr().provider("aliyun").list();
    assert_eq!(list[0].descriptor.model.as_ref(), "fun-asr-realtime");
    assert_eq!(
        list[1].descriptor.model.as_ref(),
        "fun-asr-realtime-2026-02-28"
    );
}
```

Keep existing `capability_query_filters_on_descriptor` as-is (fixture has a unique Image+thinking match → still Ok under C2).

- [ ] **Step 2: Run tests — expect fail on multi-match without default (currently silent first)**

Run: `cargo test -p orchest-provider --test selection select_errors_when_multiple_match_without_default -- --nocapture`  
Expected: FAIL (got Ok / wrong assertion).

- [ ] **Step 3: Implement C2 select**

Replace `Query::select` in `registry.rs`:

```rust
pub fn select(&self) -> Result<&'r Entry<H>, ProtocolError> {
    let matches = self.list();
    match matches.as_slice() {
        [] => Err(ProtocolError::new(
            ErrorCode::NoMatchingProvider,
            format!(
                "no registered {:?} provider matches the selection",
                self.capability
            ),
        )),
        [one] => Ok(*one),
        many => {
            let defaults: Vec<_> = many
                .iter()
                .copied()
                .filter(|e| e.descriptor.default_for_provider)
                .collect();
            match defaults.as_slice() {
                [one] => Ok(*one),
                [] | [_, _, ..] => Err(ProtocolError::new(
                    ErrorCode::NoMatchingProvider,
                    format!(
                        "ambiguous {:?} selection: {} matches without a unique default_for_provider; narrow with .id(\"provider/model\") or .provider(..)",
                        self.capability,
                        many.len()
                    ),
                )),
            }
        }
    }
}
```

Do **not** change `list()` sort semantics.

- [ ] **Step 4: Update wall doctest (C2a)**

In `crates/orchest-provider/src/lib.rs` crate docs, replace capability-only select happy path:

```rust
//! let reg = Registry::with_builtin();
//! // list-then-choose (capability-only select is ambiguous across providers):
//! let candidates = reg.chat().accepts([Modality::Text, Modality::Image]).thinking().list();
//! let _ = candidates.first();
//! // capability + identity mixed:
//! let _ = reg.asr().provider("volcengine").bidirectional().select();
//! // identity pick:
//! let _ = reg.chat().id("openai/gpt-5.4").select();
```

- [ ] **Step 5: Run selection + doctests**

Run:
```bash
cargo test -p orchest-provider --test selection -- --nocapture
cargo test -p orchest-provider --doc -- --nocapture
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/orchest-provider/src/registry.rs \
  crates/orchest-provider/src/lib.rs \
  crates/orchest-provider/tests/selection.rs
git commit -m "feat: make Query::select default-aware (C2)"
```

---

### Task 4: Asr catalog tables + model-pinned `asr_entries` (C1)

**Files:**
- Create: `crates/orchest-provider-stream/src/catalog/mod.rs`
- Create: `crates/orchest-provider-stream/src/catalog/asr.rs`
- Create: `crates/orchest-provider-http/src/catalog/asr.rs` (or sibling module under existing catalog)
- Modify: `crates/orchest-provider-stream/src/lib.rs`
- Modify: `crates/orchest-provider-http/src/lib.rs`
- Modify: `crates/orchest-provider/src/catalog.rs` (append Asr rows into materialization)
- Modify: `crates/orchest-provider/tests/selection.rs`
- Modify: `crates/orchest-provider/tests/catalog_discovery.rs`

**Interfaces:**
- Produces: static Asr `ModelRecord` tables; `asr_entries()` expands 1:1 with model-pinned factories
- Aliyun rows:
  | id | default |
  |---|---|
  | `aliyun/fun-asr-realtime` | **true** |
  | `aliyun/fun-asr-realtime-2026-02-28` | false |
- Other dialects ≥1 row each (default true, since single-row):
  - stream: volcengine/`bigmodel`, deepgram/`nova-3`, soniox/`stt-rt-v5`, elevenlabs/`scribe-v2-realtime`
  - http: assemblyai/`universal`, speechmatics/`enhanced`

**Description convention example:**

```rust
// Source: docs/external/aliyun/asr-api-doc.md
description: "阿里云 DashScope 实时语音识别（Fun-ASR），WebSocket inference 双工流式",
```

- [ ] **Step 1: Write failing selection tests for multi-model Aliyun**

```rust
#[cfg(feature = "stream")]
#[test]
fn aliyun_asr_lists_multiple_catalog_models() {
    let reg = Registry::with_builtin();
    let list = reg.asr().provider("aliyun").list();
    let models: Vec<_> = list.iter().map(|e| e.descriptor.model.as_ref()).collect();
    assert!(models.contains(&"fun-asr-realtime"));
    assert!(models.contains(&"fun-asr-realtime-2026-02-28"));
    assert!(list.len() >= 2);
}

#[cfg(feature = "stream")]
#[test]
fn aliyun_asr_provider_select_returns_fun_asr_default() {
    let reg = Registry::with_builtin();
    let picked = reg.asr().provider("aliyun").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "fun-asr-realtime");
    assert!(picked.descriptor.default_for_provider);
}

#[cfg(feature = "stream")]
#[test]
fn aliyun_asr_id_pins_fun_asr_snapshot() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .id("aliyun/fun-asr-realtime-2026-02-28")
        .select()
        .unwrap();
    assert_eq!(
        picked.descriptor.model.as_ref(),
        "fun-asr-realtime-2026-02-28"
    );
}
```

Also discovery test:

```rust
#[cfg(all(feature = "http", feature = "stream"))]
#[test]
fn discovery_lists_aliyun_streaming_asr_with_descriptions() {
    let rows: Vec<_> = list_models(ModelFilter {
        capability: Some(Capability::Asr),
        provider: Some("aliyun"),
        streaming: Some(true),
        ..Default::default()
    })
    .collect();
    assert!(rows.len() >= 2);
    for r in &rows {
        assert!(!r.description.trim().is_empty());
    }
}
```

- [ ] **Step 2: Run — expect fail (only one aliyun entry)**

Run: `cargo test -p orchest-provider --features stream --test selection aliyun_asr_ -- --nocapture`  
Expected: FAIL.

- [ ] **Step 3: Add Asr tables + pinned factories**

`stream/src/catalog/asr.rs` returns `&'static [ModelRecord]` for stream ASR models.

`stream::asr_entries()` becomes roughly:

```rust
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    catalog::asr::STREAM_ASR_MODELS
        .iter()
        .map(|rec| {
            let model = rec.model; // 'static
            let provider = rec.provider;
            Entry::new(rec.to_descriptor(), move |cfg| {
                let mut pinned = cfg.clone();
                pinned.provider = provider.to_string();
                pinned.model = model.to_string();
                let handle = match provider {
                    "aliyun" => asr::aliyun::from_provider_config(&pinned)?,
                    "volcengine" => asr::volcengine::from_provider_config(&pinned)?,
                    "deepgram" => asr::deepgram::from_provider_config(&pinned)?,
                    "soniox" => asr::soniox::from_provider_config(&pinned)?,
                    "elevenlabs" => asr::elevenlabs::from_provider_config(&pinned)?,
                    other => {
                        return Err(ProtocolError::new(
                            ErrorCode::UnknownProvider,
                            format!("no stream ASR dialect for {other}"),
                        ))
                    }
                };
                Ok(Box::new(handle) as Box<dyn Asr>)
            })
        })
        .collect()
}
```

Mirror for http batch ASR (`assemblyai`, `speechmatics`).

Keep `from_provider_config` free functions accepting arbitrary `cfg.model` for uncataloged passthrough (C1 dual path). Cataloged Entries always pin.

Append Asr records into wall `build_catalog()` under matching features so discovery matches registry feature parity.

- [ ] **Step 4: Run Asr selection + discovery tests**

```bash
cargo test -p orchest-provider --features stream --test selection aliyun_asr_ -- --nocapture
cargo test -p orchest-provider --features "http,stream" --test catalog_discovery -- --nocapture
cargo test -p orchest-provider-stream asr:: -- --nocapture
```
Expected: PASS. Existing per-provider Asr tests still pass (single-row providers remain unique; aliyun uses default).

- [ ] **Step 5: Commit**

```bash
git add crates/orchest-provider-stream/src/catalog \
  crates/orchest-provider-stream/src/lib.rs \
  crates/orchest-provider-http/src/catalog \
  crates/orchest-provider-http/src/lib.rs \
  crates/orchest-provider/src/catalog.rs \
  crates/orchest-provider/tests/selection.rs \
  crates/orchest-provider/tests/catalog_discovery.rs
git commit -m "feat: expand ASR catalog rows with model-pinned registry entries"
```

---

### Task 5: Gen multi-entry defaults (C2a A) + all-features tests

**Files:**
- Modify: `crates/orchest-provider-http/src/gen/aliyun_music.rs` (`entry_descriptor`)
- Modify: `crates/orchest-provider-visual/src/gen/volcengine.rs` (`entry_descriptor`)
- Modify: `crates/orchest-provider/tests/selection.rs`
- Modify: `.github/workflows/ci.yml`
- Optional audit note in commit body for music-gift / briefing-desk (no code change if defaults cover them)

**Default table (mandatory):**

| (capability, provider) | default model |
|---|---|
| `(GenTask, volcengine)` | `doubao-seedream-5-0-260128` |
| `(GenTask, aliyun)` | `fun-music-v1` |

- [ ] **Step 1: Write failing all-features Gen default tests**

```rust
#[cfg(all(feature = "http", feature = "visual"))]
#[test]
fn gen_aliyun_provider_select_defaults_to_fun_music() {
    let reg = Registry::with_builtin();
    let picked = reg.gen().provider("aliyun").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "fun-music-v1");
    assert!(picked.descriptor.default_for_provider);
}

#[cfg(all(feature = "http", feature = "visual"))]
#[test]
fn gen_volcengine_provider_select_defaults_to_seedream_image() {
    let reg = Registry::with_builtin();
    let picked = reg.gen().provider("volcengine").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "doubao-seedream-5-0-260128");
    assert!(picked.descriptor.default_for_provider);
}

#[cfg(all(feature = "http", feature = "visual"))]
#[test]
fn gen_defaults_unique_per_provider() {
    let reg = Registry::with_builtin();
    for provider in ["aliyun", "volcengine"] {
        let defaults: Vec<_> = reg
            .gen()
            .provider(provider)
            .list()
            .into_iter()
            .filter(|e| e.descriptor.default_for_provider)
            .collect();
        assert_eq!(
            defaults.len(),
            1,
            "{provider} must have exactly one gen default"
        );
    }
}
```

- [ ] **Step 2: Run with all weight features — expect fail until flags set**

Run:
```bash
cargo test -p orchest-provider --features "http,stream,visual" --test selection gen_ -- --nocapture
```
Expected: aliyun/volcengine provider-only select ambiguous or wrong silent winner.

- [ ] **Step 3: Mark defaults on descriptors**

```rust
// aliyun_music entry_descriptor
CapabilityDescriptor::new("aliyun", aliyun_music::DEFAULT_MODEL, Capability::GenTask)
    .with_input_modalities([Modality::Text])
    .with_output_modalities([Modality::Audio])
    .default_for_provider(true)

// volcengine image entry_descriptor
CapabilityDescriptor::new("volcengine", DEFAULT_MODEL, Capability::GenTask)
    .with_input_modalities([Modality::Text])
    .with_output_modalities([Modality::Image])
    .default_for_provider(true)
```

Leave wanx + seedance at default `false`.

**Callsite audit result (document in commit):**
- `examples/demo/music-gift/src/config.rs` — `.gen().provider(provider).build` → covered by aliyun music default when provider is aliyun; other providers remain single-row or need their own defaults later.
- `examples/demo/briefing-desk/src/media.rs` — `.asr()/.tts().provider(provider).build` → Asr multi-model only aliyun (has default); Tts still 1:1.
- Bindings (node/py) do not use Registry select for multi-entry Gen.
- No Chat defaults declared.

- [ ] **Step 4: CI all-features step**

In `.github/workflows/ci.yml` after `cargo test --workspace`:

```yaml
      - name: cargo test provider selection (all weight features)
        run: cargo test -p orchest-provider --features "http,stream,visual" --test selection --test catalog_discovery
```

This is **required** for invariant 4; default workspace test does not enable all provider features.

- [ ] **Step 5: Run verification**

```bash
cargo test -p orchest-provider --features "http,stream,visual" --test selection -- --nocapture
cargo test -p orchest-provider --features "http,stream,visual" --test catalog_discovery -- --nocapture
cargo test -p orchest-provider --features "http,stream,visual" --doc -- --nocapture
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/orchest-provider-http/src/gen/aliyun_music.rs \
  crates/orchest-provider-visual/src/gen/volcengine.rs \
  crates/orchest-provider/tests/selection.rs \
  .github/workflows/ci.yml
git commit -m "feat: declare Gen provider defaults and all-features selection CI"
```

---

### Task 6: Final verification + docs cross-link

**Files:**
- Modify: `docs/superpowers/specs/2026-08-01-multi-capability-model-catalog-design.md` (status line → Implemented Batch 0+1, or add pointer to this plan)
- Optional: one-line link from `docs/iteration/roadmap.md` if already editing nearby (not required)

- [ ] **Step 1: Full targeted regression matrix**

```bash
cargo test -p orchest-provider-core catalog::
cargo test -p orchest-provider --features http --test catalog_discovery
cargo test -p orchest-provider --features "http,stream,visual" --test selection
cargo test -p orchest-provider --features "http,stream,visual" --test catalog_discovery
cargo test -p orchest-provider-stream asr::aliyun
cargo test -p orchest-provider-http catalog::
cargo clippy -p orchest-provider -p orchest-provider-core -p orchest-provider-stream -p orchest-provider-http -p orchest-provider-visual -p orchest-protocol -- -D warnings
```

Expected: all PASS / no warnings.

- [ ] **Step 2: Spec status note**

At top of design spec, after Status line:

```markdown
**Implementation plan:** `docs/superpowers/plans/2026-08-01-multi-capability-model-catalog-batch-0-1.md` (Batch 0+1)
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-08-01-multi-capability-model-catalog-design.md
git commit -m "docs: link multi-capability catalog plan from design spec"
```

---

## Self-Review (spec coverage)

| Spec requirement | Task |
|---|---|
| ModelRecord + ModelFilter(+tools/thinking) + CatalogExt in core | Task 1 |
| LazyLock materialization; Chat projection; no dual-write (C3) | Task 2 |
| list/find + ADR-0002 Chat peel | Task 2 |
| Description non-empty invariant | Task 1–2 tests |
| Uniqueness (capability, provider, model) | Task 2 tests |
| C2 select rules | Task 3 |
| C2a capability-only / doctest | Task 3 |
| C2a Gen defaults + music-gift path | Task 5 |
| Aliyun multi-model Asr + other dialect defaults | Task 4 |
| C1 model-pinned factories | Task 4 |
| C4 description not on descriptor | Task 1–2 (only default flag on descriptor) |
| C5 Assumed for passthrough | deferred synthesis path untouched; free functions remain (document in Task 4) |
| C6 layering | Tasks 1–2, 4 |
| all-features CI | Task 5 |
| No Chat defaults until Batch 4 | Task 2 projection sets false; Task 5 audit |
| Realtime/Tts full catalogs / Chat native storage | **Out of scope** (Batch 2–4) |

**Placeholder scan:** none intentional; modality storage note resolved by recommending `Vec<Modality>` on `ModelRecord` if `'static` projection fights Chat data.

**Type consistency:** `default_for_provider` on both `ModelRecord` and `CapabilityDescriptor`; discovery via wall `catalog::{list_models,find_model,find_model_for}`; registry still uses `Entry.descriptor`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-01-multi-capability-model-catalog-batch-0-1.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — this session, executing-plans with checkpoints  

Which approach?
