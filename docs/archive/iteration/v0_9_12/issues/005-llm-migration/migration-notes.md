# Issue 005: LLM migration + dependency inversion — notes

> Records what physically moved, the inversion, and the one design deviation
> (the chat push→pull bridge). Implemented across `orchest-provider-http`,
> `agent-runtime-providers`, `agent-runtime-core`.

## Relocation map

The entire LLM impl subtree moved from `agent-runtime-providers/src` into
`orchest-provider-http/src` (via `git mv`, history preserved). Because every
internal reference is `crate::`-relative and the whole subtree moved as a unit,
no intra-impl path changed — only the 8 direct `agent_runtime_model::` imports
were repointed to `orchest_protocol::`.

| Moved | Role |
|---|---|
| `providers/{anthropic,openai,deepseek,openrouter,volcengine,minimax}` | the dialect `ChatModel` impls (`ModelAdapter` alias) |
| `registry.rs` | `ProviderFactory` / `ProviderRegistry` (the LLM factory-by-name) |
| `catalog/` | static model catalog (+ now `impl CatalogEntry for LlmModelEntry`) |
| `http.rs`, `sse/`, `role_compat.rs`, `telemetry.rs`, `defaults.rs`, `pricing.rs`, `types.rs` | shared support modules |
| `tests.rs` + `providers/*/tests.rs` | characterization suites (move with their code) |

`agent-runtime-providers` is now a one-line re-export shell
(`pub use orchest_provider_http::*;`). The two binding-facing free functions
(`create_adapter_from_config`, `normalize_provider_model`) and the `ModelAdapter`
path resolve through it with identical signatures — `node`/`py` are untouched.

## Dependency inversion

`agent-runtime-core` now depends on `orchest-protocol` directly (was
`agent-runtime-model`, the shim). Core has never depended on a concrete provider
crate in its non-dev graph; the inversion makes the spine dependency explicit and
drops the shim from core's tree ahead of its removal in Issue 008.

## Registration through the wall

`orchest_provider_http::chat_entries()` projects each **enumerable** catalog row
onto a `CapabilityDescriptor` (via the new `CatalogEntry` impl) paired with a
factory that calls `create_adapter_from_config`. The wall's
`Registry::with_builtin()` (feature `http`) now returns these; selection by
capability/identity works against real LLM descriptors. OpenRouter is a
dynamic gateway (non-enumerable) — it stays reachable only via the free-function
path, which is exactly how `node`/`py` already call it.

## Design deviation: the chat push→pull bridge

Design §1.4 sketched the convergence as a method `ChatModel::events()`. The spine
(`orchest-protocol`) holds a hard invariant — **no runtime deps** (`tokio` =
`sync` only, no scheduler). A defaulted `events()` that spawns the push
completion would force `tokio/rt` into the spine and break that invariant.

So the bridge is realized as a free function **in the impl crate**, where a
runtime is already present:

```rust
orchest_provider_http::events(model: Arc<dyn ChatModel>, messages, tools, options) -> EventStream
```

It drives the retained push `complete(.., Some(tx))` on a background task and
hands back the pulled `EventStream` (the shape ASR/TTS/realtime already speak),
surfacing a hard failure as a trailing in-band `Error` event. Same convergence,
same call ergonomics, spine stays runtime-free. `ModelAdapter`'s push transport
remains the working bridge underneath until Issue 008 flips consumers onto the
pull path. Covered by `tests/push_pull.rs`.
