# ADR-0002: Protocol-Provider Decoupling

**Status:** Accepted
**Date:** 2026-07-08 (revised and accepted 2026-07-11 after review)
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

### Problem 5: Divergence *within* a protocol

Even providers on the same wire protocol are not identical. The four
"OpenAI-compatible" adapters share the envelope (messages array,
tool-call assembly, SSE decode, `stream_options`) almost verbatim, but
each forks the **option surface** — most visibly the reasoning
extension:

- OpenAI lowers `ThinkingLevel` to `reasoning_effort`;
- DeepSeek uses a `thinking` param and replays `reasoning_content`
  into assistant history;
- Volcengine uses a differently-shaped `thinking` field and cannot
  exclude reasoning output at all;
- OpenRouter has its own `reasoning` object plus a
  `reasoning_details` replay array.

Likewise on the Messages side: Minimax needs role downgrading
(`role_compat.rs`) and speaks its own thinking dialect
(`thinking: {type, display}`) rather than Anthropic's
`budget_tokens` + signature blocks. "Provider = protocol + static
configuration" is therefore true for the transport envelope but
false for the option surface. Today that residual is handled by
duplicating the entire adapter per provider; the design below gives
it a named, bounded home instead.

## Decision

Separate **protocol** (wire dialect) from **provider** (endpoint +
auth + branding) as independent dimensions in the model string and
factory layer.

The precise formulation is **protocol core + provider profile**, not
merely "protocol + config". A provider is modeled as three layers:

1. **Protocol core** (shared code, one per wire dialect): request
   envelope construction, streaming decode, tool-call assembly,
   canonical option lowering, baseline usage extraction and error
   normalization. Contains **no provider-specific logic**.
2. **Static provider configuration** (`ProviderEntry`, pure data):
   identity, base URL, key env, supported protocols, protocol
   aliases, endpoint path overrides, extra headers. Anything that
   varies per provider but is invariant across requests and options.
3. **Provider profile** (optional, small code): a narrow, named hook
   surface for the residual behavioral divergence within a protocol —
   option lowering dialects, reasoning replay, role mapping, usage
   interpretation, option support declarations, error normalization.
   See "Provider profiles" below.

A fully protocol-compatible provider is layer 2 only (zero code). A
provider with extensions adds a small layer-3 profile. Nothing ever
adds a forked adapter or a provider conditional inside layer 1.

**Scope:** the initial implementation covers LLM chat models only
(adapters returning `Box<dyn ChatModel>`). Other capabilities served
over the same dialects (TTS, music via the Minimax dialect, etc.)
follow the same pattern later, per ADR-0001; they are out of scope
here.

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
openrouter/anthropic/claude-opus-4-8 -> openrouter provider, model
                                        "anthropic/claude-opus-4-8"
```

### Parsing rule

Model IDs may themselves contain slashes (OpenRouter's catalog format
is `openrouter/<upstream-provider>/<model-name>`, so the model name is
e.g. `anthropic/claude-opus-4-8`). Segment counting is therefore
ambiguous, and the grammar must be resolved by vocabulary, not
position:

1. Split once on `/`: the first segment is the **provider**
   (unchanged from today's `normalize_provider_model`).
2. If the remainder contains a `/`, the segment before the first `/`
   is interpreted as a **protocol** if and only if it matches a
   canonical protocol name (`messages`, `chat`, `responses`) or one
   of that provider's declared `protocol_aliases`. Otherwise the
   entire remainder is the model name.

Consequences:

- Canonical protocol names are **reserved words**: an upstream vendor
  prefix spelled exactly `messages`, `chat`, or `responses` cannot be
  expressed in the second position. No known gateway vendor collides
  with these names; the limitation is accepted and documented rather
  than worked around with escape syntax.
- Aliases are **scoped per provider**, never interpreted globally.
  `elss/anthropic/claude-sonnet-5` resolves `anthropic` -> Messages
  because the elss entry declares that alias;
  `openrouter/anthropic/claude-opus-4-8` does not — openrouter
  declares no aliases, so `anthropic/claude-opus-4-8` stays intact as
  the model name, which is what OpenRouter's API requires.

### Protocol registry

Protocols are registered independently of providers:

```rust
/// A wire protocol dialect (Messages, Chat, Responses, ...).
///
/// This enum is **chat-scoped** and lives below the ADR-0001 wall, in
/// `orchest-provider-http`. It must not appear in wall-level or
/// consumer-facing types. When other modalities adopt protocol
/// factories, protocol identity becomes a per-capability concern
/// (ADR-0001 already observes that one dialect can span LLM/TTS/music,
/// and openspeech-WS spans ASR/TTS/realtime); the identifier scheme is
/// revisited then rather than pre-designed now.
pub enum Protocol {
    /// Anthropic Messages API (/v1/messages)
    Messages,
    /// OpenAI Chat Completions API (/v1/chat/completions)
    Chat,
    /// OpenAI Responses API (/v1/responses)
    Responses,
}

/// Resolved once by the registry when a model string is parsed, and
/// passed to the protocol factory and every profile hook. Profiles
/// never rediscover identity or capability facts themselves.
pub struct ResolvedModel<'a> {
    pub provider: &'a ProviderEntry,
    pub protocol: Protocol,
    pub model: &'a str,
    /// The model's catalog entry — the canonical source of capability
    /// facts (see "Capability metadata"). `None` for dynamic-gateway
    /// models that cannot be enumerated statically.
    pub catalog: Option<&'a LlmModelEntry>,
}

/// A factory that builds an adapter for a specific protocol.
/// Protocol factories are reusable across providers.
///
/// Lives in `orchest-provider-http`, *below* the ADR-0001 wall: the
/// wall's `Entry<H>` / `Factory<H>` / `ProviderConfig` surface is
/// unchanged. Construction input is the same `ProviderConfig` the
/// wall already uses — not the retired positional
/// `(model, max_tokens, api_key, api_url)` signature that
/// `ProviderConfig` was introduced to replace — plus the
/// `ResolvedModel` carrying the identity and capability facts the
/// config does not.
pub trait ProtocolFactory: Send + Sync {
    fn protocol(&self) -> Protocol;
    fn create_adapter(
        &self,
        config: &ProviderConfig,
        resolved: &ResolvedModel<'_>,
    ) -> Result<Box<dyn ChatModel>, ProtocolError>;
}
```

(The earlier draft's separate `ProviderContext` struct is dropped: it
duplicated fields of both `ProviderConfig` and `ProviderEntry`. The
protocol factory receives the resolution result — provider entry,
protocol, model, catalog entry — as one `ResolvedModel`.)

**Caution on Responses:** unlike Messages and Chat, the Responses API
is stateful — `previous_response_id`, server-side conversation state,
built-in tools. It is recorded here as a protocol variant, but
implementing it is not merely writing a third request envelope: the
statefulness may require changes *above* the protocol layer (the
`ChatModel` trait assumes stateless request/response).

Responses is therefore **out of scope for this ADR's Chat/Messages
consolidation** (Phases 1-2). `Protocol::Responses` exists as an
identifier only — parseable and explicitly routable — with no factory
behind it. Implementing it requires a separate design review; do not
assume it slots in like Chat did.

### Provider registry (revised)

```rust
/// Value source for a provider header. Env values are read at
/// *adapter construction* time by the protocol factory — a runtime
/// env-var value cannot be `'static`, so the entry stores the env-var
/// *name*, not the value.
pub enum HeaderValue {
    Static(&'static str),
    Env(&'static str),
}

/// A provider entry: identity + protocol preferences, no adapter logic.
pub struct ProviderEntry {
    pub name: &'static str,
    /// Base URL (scheme + host, optionally a path prefix) — NOT a
    /// complete endpoint. See "URL resolution" below.
    pub default_base_url: &'static str,
    pub default_api_key_env: &'static str,
    /// Protocols this provider supports, in preference order.
    pub protocols: &'static [Protocol],
    /// Provider-scoped aliases for the protocol segment of the model
    /// string (e.g. elss: ("anthropic", Messages), ("openai", Chat)).
    pub protocol_aliases: &'static [(&'static str, Protocol)],
    /// Per-protocol endpoint path overrides for non-standard layouts
    /// (e.g. minimax: (Messages, "/anthropic/v1/messages")).
    pub path_overrides: &'static [(Protocol, &'static str)],
    /// Provider-specific headers injected into every request
    /// (e.g. openrouter: ("X-OpenRouter-Title", Env("OPENROUTER_APP_TITLE"))).
    pub extra_headers: &'static [(&'static str, HeaderValue)],
    /// Behavior profiles per protocol, for providers that deviate
    /// from protocol-canonical behavior. Empty for fully compatible
    /// providers. See "Provider profiles".
    pub profiles: &'static [(Protocol, &'static dyn ProviderProfile)],
}
```

The registry resolves `provider/[protocol/]model` into a
`ResolvedModel` (provider entry, protocol, model name, catalog entry)
per the parsing rule and hands off to the matching `ProtocolFactory`,
which looks up the provider's profile for that protocol (if any).

### Provider profiles

The named home for Problem 5's residual: per-provider behavioral
divergence *within* a protocol. A profile is attached to a
`(provider, protocol)` pair and overrides only what its provider
actually deviates on; every hook defaults to the protocol-canonical
behavior.

Every hook receives the `ResolvedModel` context (provider entry,
protocol, model name, catalog entry), so profiles read capability
facts from the resolution rather than rediscovering them from model
names.

```rust
/// Narrow, named extension surface for provider-specific behavior on
/// a protocol. All hooks have defaults (the protocol-canonical
/// behavior). Signatures are illustrative — the contract is the set
/// of named hooks, not the exact types.
pub trait ProviderProfile: Send + Sync {
    /// Lower canonical request options (ThinkingLevel, sampling, ...)
    /// onto the wire body, reporting any degradation as
    /// `OptionAdjustment`s. Default: protocol-canonical lowering
    /// (Chat: `reasoning_effort`; Messages: `thinking` budget).
    /// Examples: DeepSeek's `thinking` param; Volcengine's variant
    /// shape; OpenRouter's `reasoning` object; Minimax's
    /// `thinking: {type, display}` dialect.
    fn lower_options(
        &self,
        cx: &ResolvedModel<'_>,
        options: &RequestOptions,
        body: &mut Value,
    ) -> Vec<OptionAdjustment>;

    /// Re-inject prior assistant reasoning when replaying history.
    /// Examples: DeepSeek/Volcengine `reasoning_content`; OpenRouter
    /// `reasoning_details`.
    fn replay_reasoning(
        &self,
        cx: &ResolvedModel<'_>,
        msg: &mut Value,
        blocks: &[ContentBlock],
    );

    /// Map canonical roles onto provider-accepted roles.
    /// Example: Minimax's role downgrade (today's `role_compat.rs`).
    fn map_role(&self, cx: &ResolvedModel<'_>, role: &Role) -> WireRole;

    /// Interpret provider-specific usage reporting (cached tokens,
    /// reasoning tokens, absent usage) into canonical `TokenUsage`.
    /// Example: OpenRouter's missing-usage case, which today emits an
    /// `OptionAdjustment` and telemetry.
    fn interpret_usage(
        &self,
        cx: &ResolvedModel<'_>,
        raw: &Value,
        usage: &mut TokenUsage,
    ) -> Vec<OptionAdjustment>;

    /// Declare support for a canonical option so shared
    /// `CompatibilityPolicy` handling can degrade or error uniformly.
    /// Example: Volcengine cannot exclude reasoning output. Default:
    /// derived from `cx.catalog` where present.
    fn option_support(
        &self,
        cx: &ResolvedModel<'_>,
        option: &RequestOption,
    ) -> OptionSupport;

    /// Map provider error bodies onto canonical errors (retryability,
    /// rate-limit semantics). Default: protocol-canonical mapping.
    fn normalize_error(
        &self,
        cx: &ResolvedModel<'_>,
        status: StatusCode,
        body: &Value,
    ) -> Option<ModelError>;
}
```

**Rules — these are the load-bearing constraints of this ADR:**

1. **No provider conditionals in protocol code.** A protocol core
   containing `if provider == "deepseek"` (or matching on provider
   name in any form) is a review-blocking defect. Divergence goes in
   a profile or it doesn't go in.
2. **Compatible provider = configuration only.** A provider that
   genuinely speaks a protocol costs one `ProviderEntry` and zero
   lines of code. A provider with extensions costs a small profile.
   Nothing costs a new adapter or protocol module.
3. **No generic escape hatches.** The profile trait must never grow
   an undifferentiated transform hook such as
   `fn modify_request(&self, body: &mut Value)` — that reintroduces
   per-provider adapters behind one more layer of indirection. Hooks
   are added to the trait **by name, one at a time, when a real
   provider demonstrates the need** (this is how `map_role` got
   here: Minimax). Each hook's scope is its name; a `lower_options`
   impl that rewrites message content is out of contract.
4. **Dialect-fork threshold.** A profile may adjust option lowering,
   role mapping, usage interpretation, option support, and error
   mapping. If a provider requires a different **content-block
   encoding** or a different **stream-event shape**, that is not a
   profile — it is a different protocol, and pretending otherwise
   would hollow out the protocol core. Declare a new `Protocol`
   variant instead.

### Capability metadata

The **catalog is the canonical source of truth for model
capabilities and context limits** (thinking support, context window,
modality flags, pricing keys). Protocol cores and provider profiles
read these facts from the `ResolvedModel`'s catalog entry; they do
not maintain their own model knowledge.

Hardcoded model-name or prefix tables (today: OpenAI's
`supports_reasoning_model` and `openai_context_window` in the
adapter) are retained only as **explicitly documented fallbacks** for
models absent from the catalog — dynamic-gateway models and unlisted
previews — and must never become the primary mechanism. Volcengine's
existing pattern is the reference: consult the catalog, and treat an
unrecognised model id conservatively rather than guessing from its
name.

### URL resolution

Configured URLs — the entry's `default_base_url` and any user-supplied
`api_url` — are **base URLs**. The protocol factory appends the
protocol's canonical path (`/v1/messages`, `/v1/chat/completions`,
`/v1/responses`), or the provider's `path_overrides` entry for that
protocol when present. This is what lets Minimax become a plain
`ProviderEntry` over the Messages factory despite its non-standard
`/anthropic/v1/messages` path.

Backward-compatibility carve-out: a user-supplied `api_url` that
already ends with the resolved protocol path is used as-is (the append
is idempotent). This keeps shipped configs that set complete endpoints
(e.g. `MUSIC_GIFT_CHAT_API_URL=https://api.elss.ai/v1/messages`)
working, and replaces the ad-hoc suffix sniffing in today's elss
`resolve_api_url` with one deterministic rule.

### Auto-detection and precedence

The protocol for `provider/[protocol/]model` is resolved in this
order; the first rule that produces a supported protocol wins:

1. **Explicit segment** in the model string — a canonical protocol
   name or a provider-scoped alias. Always wins. If the named protocol
   is not in the provider's `protocols` list, resolution fails with an
   error (no silent fallback).
2. **Model-prefix table**, filtered by the provider's `protocols`
   list — a prefix match applies only if the provider supports that
   protocol:

   | Model prefix | Default protocol |
   |---|---|
   | `claude-*` | Messages |
   | everything else | Chat |

3. **Provider preference order** — if the table's pick is unsupported,
   the first protocol in the provider's `protocols` list is used. A
   Messages-only provider therefore defaults to Messages for every
   model, regardless of the table.

Responses is never auto-detected: it is opt-in via the explicit
`provider/responses/model` form until there is enough signal about
which models benefit from it (see Resolved Question 1).

### Backward compatibility

All existing model strings continue to work unchanged; no consumer
code needs to change:

- **Two-segment forms** (`anthropic/claude-sonnet-5`,
  `openai/gpt-4.1`, `deepseek/deepseek-v4-flash`): protocol is
  auto-detected.
- **Slash-containing model IDs**
  (`openrouter/anthropic/claude-opus-4-8`): `anthropic` is neither a
  canonical protocol name nor an openrouter alias, so per the parsing
  rule it stays part of the model name.
- **Shipped elss three-segment forms**
  (`elss/anthropic/claude-sonnet-5`, `elss/openai/gpt-4.1`): kept
  working via elss's provider-scoped `protocol_aliases`
  (`anthropic` -> Messages, `openai` -> Chat). The canonical
  `elss/messages/...` / `elss/chat/...` forms are preferred in docs
  going forward, but the aliases are retained indefinitely — they
  match the spelling elss's own docs use.
- **Complete-endpoint `api_url` values**
  (e.g. `https://api.elss.ai/v1/messages` from shipped
  `.env.example` files): handled by the idempotent-append rule in
  "URL resolution".

The explicit `provider/protocol/model` form is opt-in for cases where
the user needs to override the default.

## Migration Path

### Phase 1: Extract protocol factories (non-breaking)

1. Refactor existing adapters into `ProtocolFactory` implementations:
   - `MessagesProtocolFactory` (wraps current `AnthropicAdapter` logic)
   - `ChatProtocolFactory` (wraps current `OpenAiAdapter` logic)
   - `Protocol::Responses` is declared as an identifier only; no
     factory until its separate design review (see the statefulness
     caution above).
2. Extract the divergences currently forked across the four Chat
   adapters into `ProviderProfile` impls (DeepSeek/Volcengine/
   OpenRouter reasoning dialects, OpenRouter usage handling,
   Minimax role mapping and thinking dialect on Messages). This step
   is where the adapter merge actually happens; it is also the test
   of rule 3 — any divergence that can't be expressed through a
   named hook must surface as a design discussion, not a workaround.
3. Move capability facts into the catalog per "Capability metadata":
   OpenAI's `supports_reasoning_model` / `openai_context_window`
   tables become catalog entries, surviving only as documented
   fallbacks for uncataloged models.
4. `ProviderFactory::create_adapter` delegates to the protocol factory
   selected by the provider's protocol list + auto-detection.
5. All existing model strings and behavior unchanged.

### Phase 2: Add protocol to model string (non-breaking)

1. `normalize_provider_model` learns the `provider/[protocol/]model`
   form, using the parsing rule above (protocol segment recognized
   only by canonical name or provider-scoped alias, so
   slash-containing model IDs are unaffected).
2. `NormalizedProviderModel` gains a `protocol: Option<Protocol>` field.
3. Providers that only support one protocol ignore it; multi-protocol
   providers (elss, openrouter, future gateways) use it.
4. Elss's in-factory `anthropic/` / `openai/` prefix parsing moves
   into its `protocol_aliases` declaration; behavior is unchanged.

### Phase 3: Deprecate provider-specific adapters (breaking, v1.0)

1. `AnthropicAdapter`, `OpenAiAdapter`, etc. become thin wrappers or
   are removed in favor of `ProtocolFactory` + `ProviderEntry`.
2. `ProviderFactory` trait is removed or reduced to provider metadata.
3. DeepSeek, Volcengine, Minimax become `ProviderEntry` configurations
   (plus their small `ProviderProfile`s from Phase 1) over `Chat` or
   `Messages` protocol factories, not separate adapter
   implementations.

## Impact

### What changes for consumers

Nothing in Phase 1-2. Existing model strings work as-is. The
`provider/protocol/model` form is purely additive.

### What changes for SDK internals

- `ProtocolFactory` implementations live in `orchest-provider-http`,
  below the ADR-0001 wall; the wall's `Entry<H>` / `Factory<H>` /
  `ProviderConfig` surface is untouched.
- `ProviderRegistry` stores `ProviderEntry` (metadata) + dispatches to
  `ProtocolFactory` (adapter construction). The legacy
  `ProviderFactory` trait is bridged during Phase 1-2 and retired in
  Phase 3.
- `normalize_provider_model` parses the optional protocol segment per
  the parsing rule.
- `create_adapter_from_config` resolves protocol -> factory -> adapter.

### What changes for provider authors

Adding a new provider becomes declaring a `ProviderEntry` (name, base
URL, key env, supported protocols, aliases, path overrides, extra
headers) instead of implementing a full adapter. A provider with
protocol extensions adds a small `ProviderProfile` overriding only
the hooks it deviates on. Only genuinely new wire protocols — per the
dialect-fork threshold — need a new `ProtocolFactory`.

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
  `ChatProtocolFactory` with different `ProviderEntry` metadata plus
  small profiles for their reasoning dialects).
- New gateway providers = one `ProviderEntry` struct, zero adapter code.
- New protocols = one `ProtocolFactory`, available to all providers.
- Residual provider divergence gets a named, bounded home
  (`ProviderProfile`) instead of forked adapters or provider
  conditionals.
- Aligns with ADR-0001: "wire dialect is the implementation seam."

**Cons:**
- Breaking change to `ProviderFactory` trait (mitigated by Phase 1-2
  migration).
- Canonical protocol names become reserved words in the model-string
  grammar (accepted; no known vendor prefix collides).

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

## Resolved Questions

1. **OpenAI Responses API scope**: Implement after demo validation,
   before v1.0. The Responses API's value is long-conversation latency
   reduction and explicit cache control -- important for production but
   not blocking demo work. `Protocol::Responses` is added as a third
   protocol variant when implemented. Note: its stateful semantics
   (`previous_response_id`, server-side conversation state, built-in
   tools) mean it is *not* just another request envelope — see the
   caution in "Protocol registry". It is out of scope for this ADR's
   Chat/Messages consolidation and requires a separate design review
   before implementation.

2. **Gemini protocol**: Not adding. Google's Gemini wire format is not
   industry mainstream; the OpenAI-compatible endpoint covers Gemini
   access via gateways. If a direct Gemini protocol is needed later, it
   follows the same `ProtocolFactory` pattern.

3. **Pricing**: Pricing is `provider/model`, not protocol. The same
   model served by different providers has different prices (e.g.
   `anthropic/claude-sonnet-5` vs `elss/claude-sonnet-5`). Protocol
   does not affect price. Pricing stays keyed by `provider/model` —
   the catalog dimension, per "Capability metadata" — completely
   independent of the protocol factory.

4. **OpenRouter routing headers**: OpenRouter's `X-OpenRouter-Title`
   and `HTTP-Referer` are static header names with values read from
   env vars (`OPENROUTER_APP_TITLE`, `OPENROUTER_SITE_URL`) at adapter
   construction time -- not per-request dynamic. Since a runtime env
   value cannot live in a `'static` entry, `ProviderEntry` stores the
   header name plus a `HeaderValue::Static(...)` literal or
   `HeaderValue::Env(...)` env-var *name*; the protocol factory
   resolves `Env` values when it constructs the adapter. No provider
   currently needs per-request dynamic header logic.

5. **Parsing ambiguity with slash-containing model IDs** (raised in
   review, 2026-07-11): resolved by vocabulary-based parsing — the
   second segment is a protocol only if it matches a canonical
   protocol name or the provider's own alias table. See "Parsing
   rule".

6. **Shipped elss `anthropic/` / `openai/` prefixes** (raised in
   review, 2026-07-11): retained indefinitely as provider-scoped
   `protocol_aliases` on the elss entry; never interpreted globally,
   so OpenRouter's vendor-prefixed model IDs are unaffected. See
   "Backward compatibility".

7. **URL semantics** (raised in review, 2026-07-11): configured URLs
   are base URLs; protocol factories append canonical or overridden
   paths; user-supplied complete endpoints keep working via the
   idempotent-append rule. See "URL resolution".

8. **Initial scope** (raised in review, 2026-07-11): LLM chat models
   only (`Box<dyn ChatModel>`). Extending protocol factories to other
   capabilities over the same dialects is future work under the same
   pattern. The `Protocol` enum is chat-scoped and stays below the
   ADR-0001 wall.

9. **Residual divergence within a protocol** (raised in review,
   2026-07-11): "provider = protocol + static config" holds for the
   transport envelope but not the option surface — the four Chat
   adapters each fork the reasoning extension, and Minimax forks
   roles and thinking on Messages. Resolved by the `ProviderProfile`
   layer: a narrow, named hook surface attached per
   `(provider, protocol)` pair, governed by the four rules in
   "Provider profiles" (no provider conditionals in protocol code,
   config-only for compatible providers, no generic escape hatches,
   dialect-fork threshold).

10. **Where do gateway-level meta-options go** (noted in review,
    2026-07-11): options that belong to the gateway itself rather
    than to the protocol or the upstream model (e.g. OpenRouter
    routing/fallback preferences, Minimax `service_tier`) are
    neither protocol-core nor profile concerns. They ride in
    `ProviderConfig::options` and are applied by the provider's
    profile `lower_options` hook. They must not leak into canonical
    `RequestOptions`.

11. **Source of capability facts** (raised in review, 2026-07-11):
    the catalog is canonical for model capabilities and context
    limits; protocol cores and profiles read them from
    `ResolvedModel::catalog`. Hardcoded model-name/prefix tables are
    documented fallbacks for uncataloged models only, never the
    primary mechanism. See "Capability metadata".

12. **Per-protocol auth schemes** (considered in review, 2026-07-11):
    the Messages core authenticates with `x-api-key` +
    `anthropic-version`, which every current Messages provider
    (Anthropic, Minimax, Elss) accepts. A gateway demanding
    Bearer-only on Messages would make auth scheme per-entry
    configuration — deliberately **not** added now. Abstractions
    enter this design only when demonstrated by a real provider.
