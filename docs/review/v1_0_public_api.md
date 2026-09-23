# v1.0 Public API Freeze Review

**Issue:** [#309](https://github.com/jianantian/orchest/issues/309)
**Policy:** [ADR-0003](../adr/0003-release-policy.md) D2 (tiers) and D4 (SemVer)
**Status:** Approved (2026-09-23): all proposals accepted as recommended
**Date:** 2026-09-23

## Scope and method

The review covers the four Supported-tier crates: `orchest`,
`orchest-protocol`, `orchest-provider` and `orchest-storage`. For
`orchest-provider` it includes the items the crate re-exports from the
Internal crates.

The inventories live in [`v1_0_public_api/`](./v1_0_public_api/), one file
per crate. They were produced with `cargo-public-api` 0.52.0 on nightly
2026-09-22:

```bash
for c in orchest orchest-protocol orchest-provider orchest-storage; do
  cargo +nightly public-api -p "$c" --all-features -sss \
    > "docs/review/v1_0_public_api/$c.txt"
done
```

`-sss` leaves out blanket, auto-trait and auto-derived impls. The files
record the API **after** the approved changes below. #311 regenerates them
on the release candidate and expects no diff.

| Crate | enums | structs | traits | fns (incl. methods) |
| --- | ---: | ---: | ---: | ---: |
| `orchest` | 49 → 48 | 100 → 99 | 15 | 557 → 537 |
| `orchest-protocol` | 53 | 78 | 17 | 153 |
| `orchest-provider` | 0 | 8 | 0 | 92 → 88 |
| `orchest-storage` | 2 | 1 | 1 | 7 |

Each cell shows the count before and after the approved changes; a single
number means the change left it the same.

Counts come from the inventory files. Items reachable through two paths,
for example a type re-exported at the crate root, are counted once per
path.

## Findings

1. **No public type is `#[non_exhaustive]`.** That covers 68 enum
   definitions and every struct. Under D4, adding a variant or a field to
   any of them after 1.0 needs a major version.
2. **Most structs are plain data with public fields, and other crates
   build them as struct literals.** `#[non_exhaustive]` also forbids
   `..Default::default()` outside the defining crate, so marking such a
   struct breaks every literal. Worst cases, counted as literal uses outside
   the defining crate across `crates/`, `examples/` and `docs/guide/`:

   | Struct | Crate | Outside literals | Constructor |
   | --- | --- | ---: | --- |
   | `ModelResponse` | protocol | 194 | no |
   | `ModelCapabilities` | protocol | 135 | no |
   | `TokenUsage` | protocol | 107 | no |
   | `Message` | protocol | 99 | no |
   | `ToolMetadata` | orchest | 86 | yes |
   | `RequestOptions` | protocol | 77 | yes |
   | `CapabilityDescriptor` | protocol | 51 | yes |
   | `ProtocolError` | protocol | 37 | yes |
   | `ProviderRuntimeConfig` | protocol | 32 | no |
   | `ModelError` | protocol | 30 | no |

   Most of the protocol literals sit in the Internal provider crates, which
   implement the protocol.
3. **Some structs have public fields but are never built outside their
   crate.** Marking these costs nothing today. In `orchest` there are 25:
   `BundledToolDef`, `ChildRunHandle`, `CompactHookContext`, `EnvError`,
   `ExecutionContext`, `HandoffHookContext`, `HandoffInputData`,
   `HandoffResult`, `HandoffTool`, `McpError`, `McpToolDef`,
   `ModelHookContext`, `RepeatedFailureConfig`, `RunHandle`,
   `RunHookContext`, `ScanOutcome`, `ScanWarning`, `ScriptError`,
   `ScriptOutput`, `SessionPersistenceHook`, `SkillCapabilities`,
   `SkillPathEntry`, `ToolError`, `ToolHookContext`, `WebhookConfig`.
4. **Some items that are only meant for internal use are public:**
   - `orchest::bindings`: FFI helpers for `orchest-py` / `orchest-node`.
     Only the binding crates use it.
   - `orchest::telemetry` `record_*` functions and the `*_span` builders.
     Nothing outside `orchest` uses them. The `METRIC_*` name constants are
     the observability contract, so they stay public.
   - `orchest::prompts` (compaction prompt constants) and
     `orchest::tokenizer`. Nothing outside `orchest` uses them.
   - `orchest_provider::facade`: the same functions are public a second
     time as `orchest_provider::providers`.
5. **`orchest-storage` exposes `reqwest::Error` and `http::StatusCode`** in
   `ObjectStoreError`. That turns a future reqwest major upgrade into an
   `orchest-storage` major version.
6. **Public dependencies.** These third-party types appear in public
   signatures:

   | Crate | Types | Used by |
   | --- | --- | --- |
   | `serde` / `serde_json` | `Value`, `Error`, `Serializer` / `Deserializer` bounds | orchest, protocol |
   | `tokio` | `mpsc::Sender` / `Receiver`, `oneshot::Receiver`, `RwLock` | orchest, protocol |
   | `bytes` | `Bytes` | protocol |
   | `tracing` | `Span` | orchest |
   | `uuid` | `Uuid` | orchest |
   | `reqwest`, `http` | `Error`, `StatusCode` | storage (see finding 5) |

7. **Re-exported set.** `orchest-provider` re-exports these items:
   - from `orchest-provider-core`: `Entry`, `Factory`, `ProviderConfig`,
     `CatalogExt`, `ModelFilter`, `ModelRecord`, `ModelStatus`;
   - from `orchest-provider-http`: `create_adapter`,
     `create_adapter_from_config`, `normalize_provider_model`,
     `NormalizedProviderModel`, `ProviderRuntimeConfig`.

   Every one of them is used by custom provider registration
   (`Registry::register_*` takes `Entry` / `Factory` / `ProviderConfig`),
   by catalog discovery (`list_models` takes `ModelFilter` and returns
   `ModelRecord`), or by adapter construction in the bindings and examples.
   None is surplus.

## Proposals

All proposals were approved as recommended, and #309's commit applies
them. See [Decisions](#decisions).

### P1. `#[non_exhaustive]` on every public enum

Mark all 68 public enum definitions in the Supported crates.

- **Cost.** A trial build found 16 exhaustive `match`es in library crates
  that need a `_` arm: 14 in `orchest-provider-http`, 1 in
  `orchest-provider-stream` and 1 in `orchest`. The bindings, demos and
  tests stopped compiling at the first failure, so they will add a few
  more.
- **Effect.** Adding a variant becomes a minor change. Downstream users
  keep constructing variants as before, and only their exhaustive matches
  need a `_` arm.
- **Recommendation:** approve.

### P2. Struct policy

| Option | Change | Cost |
| --- | --- | --- |
| A. `#[non_exhaustive]` on every struct | Every struct with public fields needs a constructor or builder, and about 1,000 literal sites are rewritten. | Very high. It also forces the Internal crates to build protocol data through constructors. |
| **B. Tiered** (recommended) | (1) Mark the 25 structs from finding 3. (2) Freeze the literal-built data structs as exhaustive, and list them in this review as frozen: adding a field to one of them is a major change. (3) Optionally, migrate a few high-growth input structs to builders now (P2b). | Low: tier 1 is free. |
| C. Mark nothing | All structs are frozen. | None now, but nothing can grow in 1.x. |

**P2b.** These input structs are the most likely to gain options in 1.x:

- `ToolMetadata` (86 outside literals, has a constructor);
- `RequestOptions` (77, has a constructor);
- `AgentConfig` (9, has a builder);
- `RuntimeConfig` (7, has a builder).

Marking them `#[non_exhaustive]` now, and moving their literal call sites
to the constructor and `with_*` methods, would let them grow additively.
**Recommendation:** do it for `AgentConfig` and `RuntimeConfig`, which
already have builders and few literals. Defer `ToolMetadata` and
`RequestOptions`: their literal counts make this a larger change. They
stay frozen in 1.x, and any new option gets a new field only in 2.0.

### P3. Narrow internal items

| Item | Change |
| --- | --- |
| `orchest::bindings` | `#[doc(hidden)]`. It stays `pub` because the binding crates need it, and it is excluded from the SemVer promise. |
| `orchest::telemetry` `record_*`, `model_complete_span`, `tool_execute_span` | `pub(crate)`. The `METRIC_*` constants stay public. |
| `orchest::prompts`, `orchest::tokenizer` | `pub(crate)` |
| `orchest_provider::facade` | Private module. Callers use `orchest_provider::providers`. |

**Recommendation:** approve. Any of these can be made public again later,
which is an additive change.

### P4. Decouple `ObjectStoreError` from reqwest/http

Replace the `reqwest::Error` payload with a boxed
`dyn std::error::Error + Send + Sync` source, and replace `StatusCode`
with `u16`. With that change, only first-party types remain in
`orchest-storage`'s public API. **Recommendation:** approve.

### P5. Record the public dependencies

After P4, the public dependency set is `serde`, `serde_json`, `tokio`,
`bytes`, `tracing` and `uuid`. Under D4, moving any of them to a new major
version is a major change for Orchest. **Recommendation:** accept and
record it in ADR-0003.

### P6. Keep the re-exported set as is

All re-exports are needed (finding 7). **Recommendation:** record the list
in ADR-0003 D2 unchanged.

## Decisions

The owner (emile) approved every proposal as recommended on 2026-09-23.
The owner also approved each Supported crate (`orchest`,
`orchest-protocol`, `orchest-provider`, `orchest-storage`) for the 1.0
freeze with these changes applied.

| Proposal | Outcome |
| --- | --- |
| P1 | All public enums are `#[non_exhaustive]`: 48 in `orchest`, 53 in `orchest-protocol` and 2 in `orchest-storage`, counted by inventory path. The exhaustive matches in the Internal crates, bindings, demos and tests gained a `_` arm. Each arm is a deliberate fallback: unknown content blocks and media sources are dropped and recorded as an `OptionAdjustment`, unknown thinking levels map to the medium setting, and unknown roles fall back to `user`. Two infallible conversions became fallible: omni `ClientFrame: TryFrom<SessionInput>` and OpenRouter `Question: TryFrom<&DecisionQuestion>`. For a kind added later, they return `InvalidRequest` instead of guessing. |
| P2 | The 25 structs from finding 3 are `#[non_exhaustive]`. The structs in the frozen list below stay exhaustive, and under ADR-0003 D4 adding a field to one of them is a major change. Structs without public fields are opaque and can grow freely. |
| P2b | `AgentConfig` and `RuntimeConfig` are `#[non_exhaustive]`. The new `AgentConfig::new(name, ModelConfig)` is an unvalidated constructor that fills in defaults; callers then assign the public fields. Bindings, the example and the tests now build configs through it, or through `RuntimeConfig::default()` plus field assignment. `ToolMetadata` and `RequestOptions` stay frozen. |
| P3 | `orchest::bindings` is `#[doc(hidden)]` and excluded from SemVer. `orchest::telemetry` `record_*`, `model_complete_span` and `tool_execute_span` are `pub(crate)`, and the `METRIC_*` constants stay public. `orchest::prompts` and `orchest::tokenizer` are `pub(crate)`. `orchest_provider::facade` is private, and callers use `orchest_provider::providers`. |
| P4 | `ObjectStoreError::Transport` holds a boxed `dyn Error + Send + Sync` source, with no `From<reqwest::Error>` impl. `ObjectStoreError::Rejected::status` is a `u16`. `orchest-storage` no longer names `reqwest` or `http` types. |
| P5 | Public dependencies: `serde`, `serde_json`, `tokio`, `bytes`, `tracing`, `uuid`. Recorded in ADR-0003 D4. |
| P6 | The re-exported set is recorded in ADR-0003 D2, unchanged. |

### Frozen structs (exhaustive in 1.x)

These structs have public fields and are built as struct literals outside
their crate:

- **`orchest`:** `BudgetConfig`, `BudgetUsage`, `CompactionConfig`,
  `CompletionRequest`, `Handoff`, `JobHandle`, `LoopDetectionConfig`,
  `McpServerConfig`, `ModelConfig`, `RepeatedFailureHookContext`,
  `RetryPolicy`, `RunState`, `SessionSnapshot`, `SkillDependencies`,
  `SkillManifest`, `SkillsConfig`, `ToolCall`, `ToolContext`,
  `ToolMetadata`.
- **`orchest-protocol`:** `BooleanCriteria`, `CacheCapability`,
  `CapabilityDescriptor`, `ChatCapabilityExt`, `DecisionRequest`,
  `DecisionResponse`, `DecisionUsage`, `GenHandle`, `GenRequest`,
  `GenResult`, `Message`, `ModelCapabilities`, `ModelError`,
  `ModelPricing`, `ModelResponse`, `ModelSpec`, `MusicParams`,
  `OptionAdjustment`, `PricingRates`, `PricingTier`, `ProtocolError`,
  `ProviderRuntimeConfig`, `RealtimeHandle`, `ReasoningCapability`,
  `RequestOptions`, `SegmentRef`, `StreamingTranscribeRequest`,
  `SynthesizeRequest`, `SynthesizeResult`, `TimedSegment`, `TimedText`,
  `TokenUsage`, `ToolDef`, `TrackMeta`, `TranscribeRequest`,
  `TranscribeResult`, `UpstreamErrorDetail`.
- **`orchest-provider`:** `DecisionConfig`.
- **`orchest-storage`:** `ObjectStoreConfig`.

### Verification

On the changed tree the following all pass:

- `cargo fmt --check`;
- `cargo clippy --workspace -- -D warnings`;
- `cargo test --workspace` (1216 tests);
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`;
- `cargo build --examples --workspace`;
- `scripts/lint-check.sh`;
- the MSRV check on 1.87;
- the Python binding tests (22) and the Node SDK tests (8).
