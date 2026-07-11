# ADR-0002: Protocol-Provider Decoupling

**Status:** Proposed
**Date:** 2026-07-08
**Deciders:** Orchest maintainer (emile)
**Related:** [ADR-0001](./0001-provider-unification.md), Elss provider registration

## Context

ADR-0001 established that **wire dialect is the implementation seam, not
vendor or modality**. The registry/umbrella wall decouples consumer-side
selection (capability/identity) from implementation-side dialects.

However, the current `ProviderFactory` implementation violates this
principle. Each factory is named after a provider and hard-wired to a
single adapter type:

```
AnthropicFactory  -> AnthropicAdapter   (Messages API)
OpenAiFactory      -> OpenAiAdapter      (Chat Completions API)
DeepSeekFactory    -> DeepSeekAdapter    (OpenAI-compat, same wire as OpenAi)
OpenRouterFactory  -> OpenRouterAdapter  (OpenAI-compat, with routing headers)
VolcengineFactory  -> VolcengineAdapter  (OpenAI-compat, Ark endpoint)
MinimaxFactory     -> MinimaxAdapter     (Anthropic-compat, /anthropic/v1/messages)
ElssFactory        -> routes to Anthropic or OpenAI adapter internally
```

This creates several problems:

### Problem 1: Protocol is bound to provider name

A consumer who wants "the Anthropic Messages protocol" must name the
`anthropic` provider. But Minimax also speaks the Messages protocol
(`api.minimaxi.com/anthropic/v1/messages`), and Elss proxies it. The
consumer cannot say "use Messages protocol against endpoint X" without
knowing which provider name happens to map to that protocol.

### Problem 2: Gateway providers need ad-hoc routing

The Elss factory (added 2026-07-08) works around the binding by parsing
the model string for protocol hints (`elss/anthropic/model` vs
`elss/openai/model`) and delegating to `AnthropicAdapter` or
`OpenAiAdapter`. This logic lives inside `ElssFactory` and cannot be
reused by the next gateway provider.

### Problem 3: New protocols require new providers

OpenAI's Responses API (`/v1/responses`) is a third wire protocol
(distinct from Chat Completions). Under the current design, supporting
it means either:
- Adding an `OpenAiResponsesFactory` (duplicating auth/config), or
- Hacking it into `OpenAiFactory` with a model-string flag

Neither is correct. The Responses API is a **protocol** that any
provider could expose, not an OpenAI-specific feature.

### Problem 4: Same provider, multiple protocols

Elss, OpenRouter, and future gateways support multiple protocols for
the same model. A user may want `claude-sonnet-5` via Anthropic
Messages (for prompt caching) or via OpenAI Chat (for tool-use
parity). The choice is a **protocol selection**, not a provider
selection.

## Decision

Separate **protocol** (wire dialect) from **provider** (endpoint +
auth + branding) as independent dimensions in the model string and
factory layer.

### Model string format

```
provider/model              -- protocol auto-detected (backward compatible)
provider/protocol/model     -- explicit protocol
```

Examples:
```
anthropic/claude-sonnet-5            -> anthropic provider, auto protocol
elss/claude-sonnet-5                 -> elss provider, auto -> messages
elss/messages/claude-sonnet-5        -> elss provider, explicit messages
elss/chat/gpt-4.1                    -> elss provider, explicit chat
openai/responses/gpt-5.4             -> openai provider, explicit responses
```

### Protocol registry

Protocols are registered independently of providers:

```rust
/// A wire protocol dialect (Messages, Chat, Responses, ...).
pub enum Protocol {
    /// Anthropic Messages API (/v1/messages)
    Messages,
    /// OpenAI Chat Completions API (/v1/chat/completions)
    Chat,
    /// OpenAI Responses API (/v1/responses)
    Responses,
}

/// A factory that builds an adapter for a specific protocol.
/// Protocol factories are reusable across providers.
pub trait ProtocolFactory: Send + Sync {
    fn protocol(&self) -> Protocol;
    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
        provider_context: &ProviderContext,
    ) -> Result<Box<dyn ChatModel>, ModelError>;
}

/// Provider-specific context passed to the protocol factory.
pub struct ProviderContext {
    pub provider_name: String,
    pub default_api_url: String,
    pub default_api_key_env: String,
    /// Provider-specific headers (e.g. OpenRouter's X-Title)
    pub extra_headers: Vec<(String, String)>,
}
```

### Provider registry (revised)

```rust
/// A provider entry: identity + protocol preferences, no adapter logic.
pub struct ProviderEntry {
    pub name: &'static str,
    pub default_api_url: &'static str,
    pub default_api_key_env: &'static str,
    /// Protocols this provider supports, with auto-detection order.
    pub protocols: &'static [Protocol],
    /// Provider-specific headers injected into every request.
    pub extra_headers: &'static [( &'static str, &'static str)],
}
```

The registry resolves `provider/model` -> `(ProviderEntry, Protocol,
model)` and hands off to the matching `ProtocolFactory`.

### Auto-detection

When no explicit protocol is given, auto-detect from the model name:

| Model prefix | Default protocol |
|---|---|
| `claude-*` | Messages |
| `gpt-*`, `o3-*`, `o4-*` | Chat (Responses for reasoning models) |
| `deepseek-*` | Chat |
| `gemini-*` | Chat |
| everything else | Chat |

Providers can override the auto-detection order (e.g. a provider that
only supports Messages would default to Messages for all models).

### Backward compatibility

Existing model strings (`anthropic/claude-sonnet-5`,
`openai/gpt-4.1`, `deepseek/deepseek-v4-flash`) continue to work
unchanged -- the protocol is auto-detected. No consumer code needs to
change.

The explicit `provider/protocol/model` form is opt-in for cases where
the user needs to override the default.

## Migration Path

### Phase 1: Extract protocol factories (non-breaking)

1. Refactor existing adapters into `ProtocolFactory` implementations:
   - `MessagesProtocolFactory` (wraps current `AnthropicAdapter` logic)
   - `ChatProtocolFactory` (wraps current `OpenAiAdapter` logic)
   - `ResponsesProtocolFactory` (new, for OpenAI Responses API)
2. `ProviderFactory::create_adapter` delegates to the protocol factory
   selected by the provider's protocol list + auto-detection.
3. All existing model strings and behavior unchanged.

### Phase 2: Add protocol to model string (non-breaking)

1. `normalize_provider_model` learns the `provider/protocol/model` form.
2. `NormalizedProviderModel` gains a `protocol: Option<Protocol>` field.
3. Providers that only support one protocol ignore it; multi-protocol
   providers (elss, openrouter, future gateways) use it.

### Phase 3: Deprecate provider-specific adapters (breaking, v1.0)

1. `AnthropicAdapter`, `OpenAiAdapter`, etc. become thin wrappers or
   are removed in favor of `ProtocolFactory` + `ProviderContext`.
2. `ProviderFactory` trait is removed or reduced to provider metadata.
3. DeepSeek, Volcengine, Minimax become `ProviderEntry` configurations
   over `Chat` or `Messages` protocol factories, not separate adapter
   implementations.

## Impact

### What changes for consumers

Nothing in Phase 1-2. Existing model strings work as-is. The
`provider/protocol/model` form is purely additive.

### What changes for SDK internals

- `ProviderFactory` trait gains a `ProviderContext` parameter.
- `ProviderRegistry` stores `ProviderEntry` (metadata) + dispatches to
  `ProtocolFactory` (adapter construction).
- `normalize_provider_model` parses optional protocol segment.
- `create_adapter_from_config` resolves protocol -> factory -> adapter.

### What changes for provider authors

Adding a new provider becomes declaring a `ProviderEntry` (name, URL,
key env, supported protocols, extra headers) instead of implementing
a full adapter. Only genuinely new wire protocols need a new
`ProtocolFactory`.

### What changes for gateway providers (elss, openrouter, ...)

No more ad-hoc routing. The provider declares which protocols it
supports; the consumer picks one via the model string or gets
auto-detection.

## Options Considered

### Option A: Keep provider-bound adapters, add protocol to model string

Model string: `provider/protocol/model`, but each provider factory
still owns its adapter construction and switches internally.

**Pros:** Minimal change to existing code.
**Cons:** Doesn't solve the core problem -- protocol logic is still
duplicated across providers. Adding a new protocol means touching every
provider factory. DeepSeek and Volcengine still carry near-identical
OpenAI-compat adapters.

### Option B: Protocol registry + provider metadata (CHOSEN)

Protocols are first-class, registered independently. Providers are
metadata configurations over protocols.

**Pros:**
- Kills adapter duplication (DeepSeek/Volcengine/OpenRouter all use
  `ChatProtocolFactory` with different `ProviderContext`).
- New gateway providers = one `ProviderEntry` struct, zero adapter code.
- New protocols = one `ProtocolFactory`, available to all providers.
- Aligns with ADR-0001: "wire dialect is the implementation seam."

**Cons:**
- Breaking change to `ProviderFactory` trait (mitigated by Phase 1-2
  migration).
- `ProviderContext` adds a parameter to factory methods.

### Option C: Full protocol-first, no provider registry

Model string: `protocol/model`, provider is just `api_url` + `api_key`
config. No provider name at all.

**Pros:** Simplest model string; maximum flexibility.
**Cons:** Loses provider identity (needed for pricing, telemetry,
capability catalog, default URL/key resolution). Breaks all existing
call sites. Too far from current architecture.

## Trade-off Analysis

The decisive observation echoes ADR-0001: **protocol (wire dialect) is
the implementation seam, provider is a label.** The current code says
the right thing in ADR-0001 but violates it in the factory layer.

Option B is the natural completion of ADR-0001's architecture: the
registry wall already decouples consumer selection from implementation;
this change makes the implementation side also protocol-oriented rather
than provider-oriented.

The migration is non-breaking in Phase 1-2 (all existing model strings
and APIs continue to work), with the cleanup in Phase 3 deferred to
v1.0 when breaking changes are acceptable.

## Open Questions

1. **OpenAI Responses API scope**: Should `Protocol::Responses` be
   implemented in this iteration or deferred? It's a new protocol with
   different request/response shapes from Chat Completions.

2. **Gemini protocol**: Google's Gemini API has its own wire format
   (not OpenAI-compatible). Should `Protocol::Gemini` be added now or
   when a consumer needs it?

3. **Provider-specific pricing/catalog**: DeepSeek and Volcengine have
   provider-specific pricing tables. These stay in the provider entry,
   not the protocol factory. Confirm this doesn't create awkward
   coupling.

4. **OpenRouter routing headers**: OpenRouter needs `X-Title` and
   `Site-Url` headers. These fit naturally in `ProviderContext::extra_headers`.
   Confirm no other provider needs per-request dynamic header logic.
