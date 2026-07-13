# v0.12 migration notes — ADR-0002 Phase 3 (breaking)

> Records the public symbols v0.12 **removes** from `orchest-provider-http` and the
> construction path that replaces each. v0.12 is the last pre-freeze iteration; v1.0
> freezes the surface this iteration leaves. Behavior is unchanged — every provider's
> request/response bytes and degrade adjustments are byte-identical to v0.11.

## What Phase 3 removes

Phase 3 completes the protocol-provider decoupling: the wire protocol (Chat
`/v1/chat/completions`, Messages `/v1/messages`) is the unit of code, and provider
identity is data (a `ProviderEntry` + an optional `ProviderProfile`). The transitional
per-vendor adapter/factory types are gone.

| Removed symbol | Kind | Replacement |
|---|---|---|
| `ProviderFactory` trait + all `*Factory` structs | construction | the entry surface (`create_adapter` / `create_adapter_from_config`), slice 001 |
| `AnthropicAdapter`, `AnthropicConfig` | Messages adapter | shared `MessagesAdapter` (internal), reached via the entry surface |
| `MinimaxAdapter`, `MinimaxConfig` | Messages adapter | shared `MessagesAdapter` (internal), reached via the entry surface |
| `OpenAiConfig`, `DeepSeekConfig`, `VolcengineConfig`, `OpenRouterConfig` and their `*Adapter`s | Chat adapters | shared `ChatAdapter` (internal), reached via the entry surface (removed in slice 002 Chat) |
| `orchest_provider_http::providers::*` (module now `pub(crate)`) | impl modules | not a supported surface — construct through the entry surface below |

The four Chat providers (OpenAI/DeepSeek/Volcengine/OpenRouter) collapsed onto one
`ChatAdapter`; the two Messages providers (Anthropic/Minimax) onto one `MessagesAdapter`.
Both adapters are crate-internal. Every provider is now `ProviderEntry` + a
`ProviderProfile` carrying only that provider's divergence (option support, capability
facts, SSE reasoning fields, role mapping, multimodal encoding, auth headers).

## How to construct now

There was never a supported downstream path that named a `*Adapter`/`*Config`/`*Factory`
directly — bindings and consumers go through the wall (`orchest-provider`), which
re-exports only free functions and config/registry types:

```rust
// Preferred: the wall's free functions (unchanged signatures across v0.11 → v0.12).
use orchest_provider::{create_adapter_from_config, ProviderRuntimeConfig};

let model = create_adapter_from_config(ProviderRuntimeConfig {
    model: "anthropic/claude-sonnet-5".into(), // provider/model grammar
    api_key: Some(key),
    api_key_env: None,
    api_url: None,
    max_tokens: Some(4096),
})?;
```

- The `provider/model` string grammar and explicit `provider/protocol/model` form are
  unchanged (slices 008/010).
- Capability-based selection still goes through the wall registry
  (`orchest_provider_http::chat_entries()` / `asr_entries()` / `tts_entries()` /
  `gen_entries()`), unchanged.
- A downstream that previously built a `*Adapter` directly (none in this workspace)
  switches to `create_adapter_from_config` (or `create_adapter`) with the same
  `model` / `api_key` / `api_url` / `max_tokens` inputs the old `*Config` carried.

## Surface audit (this workspace)

The wall (`orchest-provider`) re-exports only `create_adapter`,
`create_adapter_from_config`, `normalize_provider_model`, `NormalizedProviderModel`,
`ProviderRuntimeConfig`, and the `*_entries()` registration functions — no impl adapter,
config, or profile type. `orchest`, `orchest-node`, and `orchest-py` name only protocol +
wall symbols; none referenced a removed type, and all build and test green. The frozen
v1.0 provider surface is therefore the entry + protocol-factory surface, with no
`*Factory` / `*Adapter` / `*Config` types.
