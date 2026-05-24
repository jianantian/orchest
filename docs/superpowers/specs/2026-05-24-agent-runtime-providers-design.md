# agent-runtime-providers crate design

> Date: 2026-05-24
> Status: Review

## Goal

Provide a unified LLM interface across providers and models. Users should express requests in Orchest's provider-neutral concepts — conversation, tools, reasoning, streaming, usage, caching, and continuation — while adapters translate those concepts into provider-specific request and response formats.

Extract LLM provider logic from `agent-runtime-core` into a standalone `agent-runtime-providers` crate. The crate must be independently usable — `cargo add agent-runtime-providers` gives users a complete LLM calling library without pulling in the agent runtime (run loop, tools, skills, MCP, budget, etc.).

Initial providers: Anthropic, OpenAI, DeepSeek, OpenRouter.

## Unification principles

The providers crate is not a thin HTTP wrapper. It is a normalization layer with a single mental model:

- **Intent in, normalized events out.** Callers pass provider-neutral `Message`, `ToolDef`, and `RequestOptions`; adapters emit provider-neutral `StreamEvent`s and return `ModelResponse`.
- **Provider details are adapter-private by default.** Anthropic `thinking`, OpenAI `reasoning_effort`, DeepSeek `thinking`, and OpenRouter `reasoning` are implementation details unless preserved as opaque continuation metadata.
- **No silent semantic loss.** If a provider cannot honor a requested option exactly, the adapter either records an `OptionAdjustment` in the response or returns a stable `ModelError` when strict compatibility is requested.
- **No silent provider failures.** Adapters may normalize provider errors, but must preserve upstream status codes, error codes, messages, and raw payloads when available. Debuggability beats pretty errors.
- **Exact replay beats normalization.** Normalized fields are for user-facing APIs, logs, and compaction; provider-native replay metadata is preserved exactly when required for multi-turn or tool-call continuation.
- **Core never branches by provider.** The agent runtime depends only on the normalized contract. Provider-specific behavior belongs inside adapters and provider-crate tests.

## Dependency direction

```
agent-runtime-providers/     <-- standalone, zero workspace-internal dependencies
  owns: ModelAdapter trait + public types + 4 adapters

agent-runtime-core/          <-- depends on providers
  re-exports provider types
  owns: run loop, tools, skills, MCP, budget, events

agent-runtime-py / node      <-- depends on core (gets providers transitively)
```

**Rule: providers never depends on any workspace-internal crate.** It is always a leaf node. This guarantees standalone usability and prevents circular dependencies.

## Public types

All types below are defined and owned by providers. Core re-exports them.

### ModelAdapter trait

```rust
#[async_trait]
pub trait ModelAdapter: Send + Sync {
    /// The provider name (e.g. "anthropic", "openai", "deepseek", "openrouter").
    /// Used for logging, telemetry, and token usage attribution.
    fn provider_name(&self) -> &str;

    /// The model identifier this adapter was configured with
    /// (e.g. "claude-sonnet-4-20250514", "gpt-4o").
    fn model_name(&self) -> &str;

    /// Normalized capabilities for the configured model.
    /// Callers use this for UI, validation, and strict compatibility checks
    /// without knowing provider-specific API parameters.
    fn capabilities(&self) -> ModelCapabilities;

    /// Send a model request. Always returns the complete ModelResponse.
    ///
    /// When `tx` is `Some`, emits StreamEvent items during streaming
    /// (text deltas, thinking deltas, tool call chunks, done).
    /// When `tx` is `None`, still processes the full response internally
    /// and returns ModelResponse — no streaming events are emitted.
    ///
    /// This unified signature ensures stream and non-stream paths share
    /// the same implementation. Callers choose behavior by passing or
    /// omitting the channel.
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError>;
}
```

`ModelAdapter::complete()` is the actual provider interface. Convenience free functions are provided for the common chat use cases:

```rust
/// Streaming helper — creates a channel and returns (response_future, receiver).
pub fn stream_chat(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &RequestOptions,
) -> (impl Future<Output = Result<ModelResponse, ModelError>> + '_, mpsc::Receiver<StreamEvent>) {
    let (tx, rx) = mpsc::channel(64);
    let fut = adapter.complete(messages, tools, options, Some(tx));
    (fut, rx)
}

/// Non-streaming helper — discards events, returns only the final response.
pub async fn chat(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &RequestOptions,
) -> Result<ModelResponse, ModelError> {
    adapter.complete(messages, tools, options, None).await
}
```

Callers of `stream_chat()` must poll the returned receiver while the response future is running. The event channel is bounded; awaiting the response future without draining events can apply backpressure and stall the provider request. Higher-level SDKs may wrap this pair as a single async stream to make the driving behavior harder to misuse.

### Message types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    Thinking {
        /// Human-readable reasoning text when the provider exposes it.
        /// Providers that only return opaque reasoning metadata may leave
        /// this empty while still preserving `provider_details`.
        text: Option<String>,
        /// Opaque signature for multi-turn thinking continuity.
        /// Anthropic requires thinking blocks to be passed back unchanged
        /// with their signature intact. Must be preserved exactly when
        /// replaying assistant messages.
        signature: Option<String>,
        /// Provider-native reasoning payload that must be preserved exactly.
        /// Examples:
        /// - OpenRouter `reasoning_details`
        /// - DeepSeek `reasoning_content` replay metadata around tool calls
        /// - OpenAI encrypted reasoning payloads, when exposed by an adapter
        provider_details: Option<Value>,
    },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: Value },
}
```

`ContentBlock::Thinking` is new compared to the current core types. It persists thinking / reasoning continuity data in the message history rather than only surfacing it through streaming events. This block intentionally has both normalized fields (`text`, `signature`) and an opaque `provider_details` field. The normalized fields are useful for logs, compaction, and UI; `provider_details` is the source of truth when a provider requires exact replay.

Provider adapters must preserve reasoning metadata exactly when the upstream protocol requires it:

- Anthropic signed thinking blocks must keep their signature intact.
- OpenRouter `reasoning_details` must not be reordered, summarized, or partially replayed.
- DeepSeek `reasoning_content` must be replayed for assistant turns with tool calls, per its thinking-mode contract.

This enables:

- Context compaction that preserves reasoning traces
- Sub-agent message forwarding that includes thinking
- Session persistence of thinking blocks
- Multi-turn thinking continuity (via signature preservation)

### Thinking, output, and caching configuration

Orchest exposes one normalized reasoning/thinking concept even though providers name it differently (`thinking`, `reasoning`, `reasoning_effort`, `reasoning_details`). Public options describe user intent; adapter sections document how that intent maps to concrete APIs.

Thinking has three independent concerns:

1. **是否思考 (whether to think)** — binary on/off, controlled by `ThinkingLevel::Off` vs any other level
2. **怎么思考 (how to think)** — depth/effort, controlled by `ThinkingLevel` granularity + optional `budget_tokens`
3. **输出是否包含思考 (whether output includes thinking)** — independent of #1 and #2, controlled by `include_thinking`

These are orthogonal: a model can think deeply but exclude thinking from the output (saves streaming latency), or think minimally but still include it (for transparency).

```rust
/// Controls thinking depth — combines "whether" and "how deep".
/// Each adapter maps these levels to provider-specific parameters.
/// Not all providers support all levels; mismatches are handled through
/// RequestOptions::compatibility_policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ThinkingLevel {
    /// Don't think. No reasoning tokens produced.
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
}

/// Cache policy hint. Adapters map to provider-specific mechanisms.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CachePolicy {
    /// No caching preference.
    None,
    /// Short-lived cache (Anthropic: 5-min ephemeral; others: automatic).
    #[default]
    Auto,
    /// Provider-specific extended retention. Examples: Anthropic 1-hour TTL,
    /// OpenAI 24-hour prompt cache retention when supported.
    Long,
}

/// How an adapter learned a capability value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CapabilitySource {
    /// Static table maintained by Orchest for known models.
    #[default]
    Static,
    /// Provider metadata endpoint or SDK model listing.
    ProviderMetadata,
    /// Conservative fallback for unknown models.
    Assumed,
}

/// Controls how adapters handle provider/model capability mismatches.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    /// Prefer a working request. Provider/model limitations are normalized
    /// to the closest documented behavior and reported in ModelResponse.
    #[default]
    Coerce,
    /// Fail when the provider/model cannot honor a requested option exactly.
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    pub streaming: bool,
    pub tool_use: bool,
    pub parallel_tool_use: bool,
    pub reasoning: ReasoningCapability,
    pub prompt_cache: CacheCapability,
    pub max_output_tokens: Option<u32>,
    pub context_window_size: Option<u64>,
    pub source: CapabilitySource,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReasoningCapability {
    pub supported: bool,
    pub efforts: Vec<ThinkingLevel>,
    pub budget_tokens: bool,
    pub output_exclusion: bool,
    pub replay_metadata_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheCapability {
    pub supported: bool,
    pub explicit_breakpoints: bool,
    pub long_ttl: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionAdjustment {
    /// Stable path into RequestOptions, e.g. "thinking" or "temperature".
    pub option: String,
    /// JSON representation of what the caller requested.
    pub requested: Value,
    /// JSON representation of what the adapter sent or effectively applied.
    pub applied: Value,
    /// Stable reason code, e.g. "unsupported_effort", "ignored_in_reasoning_mode".
    pub reason: String,
}
```

### RequestOptions

```rust
/// Per-request options passed to `ModelAdapter::complete()`.
/// The same options struct is used for both streaming and non-streaming calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestOptions {
    // --- 是否思考 + 怎么思考 (thinking behavior) ---

    /// Thinking depth. Default: `Medium`.
    pub thinking: ThinkingLevel,

    /// Advanced: override the provider's default token budget for this
    /// thinking level. Most models do not support explicit budget control.
    /// Anthropic honors this only for `thinking.type: "enabled"`; adaptive
    /// thinking uses effort instead. OpenRouter forwards it as
    /// `reasoning.max_tokens` when the routed model supports that field.
    /// Ignored when `thinking` is `Off`.
    pub thinking_budget_tokens: Option<u32>,

    // --- 输出是否包含思考 (thinking output) ---

    /// Whether to include thinking content in the output. Independent
    /// of `thinking` level — the model can think but exclude thinking
    /// from stream events and ModelResponse.content.
    ///
    /// When `true` (default): thinking content appears in StreamEvent
    /// (ThinkingStart/Thinking/ThinkingEnd) and in ModelResponse.content
    /// as ContentBlock::Thinking.
    ///
    /// When `false`: request hidden reasoning when the provider supports
    /// output exclusion. If a provider can only hide reasoning by disabling
    /// reasoning, behavior is controlled by `compatibility_policy`.
    ///
    /// Provider mapping:
    /// - Anthropic: maps to `thinking.display` ("summarized" vs "omitted")
    /// - OpenAI: reasoning tokens are never exposed (always false effectively)
    /// - DeepSeek: output exclusion is unsupported; Coerce disables thinking
    ///   with an OptionAdjustment, Strict returns an error
    /// - OpenRouter: maps to `reasoning.exclude`
    pub include_thinking: bool,

    /// How strictly provider/model capability mismatches should be handled.
    /// Default: Coerce and report adjustments in ModelResponse.
    pub compatibility_policy: CompatibilityPolicy,

    // --- Other options ---

    /// Per-request max output tokens override. Falls back to the
    /// adapter's configured default when `None`.
    pub max_tokens: Option<u32>,

    /// Sampling temperature (0.0 – 2.0 for most providers).
    /// Higher values increase randomness. When `None`, the provider's
    /// default applies. Not all providers support the full range.
    pub temperature: Option<f32>,

    /// Nucleus sampling. Only consider tokens whose cumulative
    /// probability exceeds `top_p`. Mutually exclusive with
    /// `temperature` adjustments on some providers.
    pub top_p: Option<f32>,

    /// Prompt cache policy. Default: `Auto` (provider-managed caching).
    pub cache_policy: CachePolicy,
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            thinking: ThinkingLevel::default(),
            thinking_budget_tokens: None,
            include_thinking: true,
            compatibility_policy: CompatibilityPolicy::default(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            cache_policy: CachePolicy::default(),
        }
    }
}
```

### ThinkingLevel provider mapping

ThinkingLevel is the user-facing control for dimensions #1 and #2. Each adapter maps it to provider-specific API parameters:

| Provider   | API parameter | Level mapping | budget_tokens |
|------------|--------------|---------------|---------------|
| **Anthropic** | `thinking.type`, `thinking.budget_tokens`, `thinking.display`, `output_config.effort` | Off→`disabled`; newer models use `adaptive` + `output_config.effort`; older extended-thinking models use `enabled` + budget tokens. See adapter section for details. | ✅ Supported only in `enabled` mode. Ignored in `adaptive` mode with an `OptionAdjustment`. |
| **OpenAI** | `reasoning_effort` | Off→`"none"` when supported; Minimal→`"minimal"` when supported else `"low"`; Low→`"low"`; Medium→`"medium"`; High→`"high"`; XHigh→`"xhigh"` when supported else `"high"` | ❌ Ignored |
| **DeepSeek** | `thinking.type` + `reasoning_effort` | Off→`thinking.type: "disabled"`; Minimal/Low/Medium/High→`enabled` + `"high"`; XHigh→`enabled` + `"max"` | ❌ Ignored |
| **OpenRouter** | `reasoning.effort` or `reasoning.max_tokens` | Sends exactly one of `effort` or `max_tokens`: explicit `thinking_budget_tokens` wins, otherwise effort maps directly (none/minimal/low/medium/high/xhigh). | ✅ Sent as `reasoning.max_tokens`; cannot be combined with `reasoning.effort`. |

### include_thinking provider mapping

`include_thinking` controls dimension #3 independently:

| Provider   | `true` (default) | `false` |
|------------|-------------------|---------|
| **Anthropic** | `thinking.display: "summarized"` — thinking streamed and in response | `thinking.display: "omitted"` — no thinking in stream, signature-only block in response |
| **OpenAI** | No effect — OpenAI never exposes reasoning tokens to callers | No effect |
| **DeepSeek** | `reasoning_content` included in response and streamed | Unsupported as pure output exclusion. Coerce disables thinking and records `output_exclusion_unsupported_disables_reasoning`; Strict errors. |
| **OpenRouter** | `reasoning.exclude: false` — thinking included | `reasoning.exclude: true` — thinking excluded from response |

When a provider/model doesn't support a requested level, behavior is controlled by `RequestOptions::compatibility_policy`:

- `Coerce`: clamp or omit only when that preserves the closest documented behavior, and include an `OptionAdjustment` in `ModelResponse.option_adjustments`.
- `Strict`: return a stable `ModelError` such as `code: Some("unsupported_reasoning_level")`.

### Tool definition

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

pub type JsonSchema = serde_json::Value;
```

ToolDef stays with the model layer because it describes what gets sent to the LLM. The runtime's richer tool metadata (side_effect, requires_approval, timeout, etc.) remains in core.

### Message serialization contract

`Message` is Orchest's normalized conversation shape. Adapters own all provider-specific serialization:

- `Role::System`: Anthropic serializes these blocks into the top-level `system` parameter because Messages API has no input `"system"` role. OpenAI serializes to `developer` when the target model family prefers developer messages, otherwise `system`; DeepSeek and OpenRouter serialize as Chat Completions `system` messages.
- `Role::Tool`: Anthropic serializes `ContentBlock::ToolResult` into user-turn `tool_result` content blocks. OpenAI-compatible providers serialize tool results as `role: "tool"` messages with `tool_call_id`.
- Assistant `ContentBlock::ToolUse`: Anthropic serializes to `tool_use` content blocks. OpenAI-compatible providers serialize to assistant `tool_calls`.
- `ContentBlock::Thinking`: adapters serialize only the fields the provider accepts, while preserving provider-native replay metadata exactly when required.

Core and SDK callers must not build provider-native message JSON themselves.

### Streaming events

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    Text { delta: String },
    ThinkingStart,
    Thinking { delta: String },
    /// Signals the end of a thinking block. `signature` carries the
    /// opaque token needed for multi-turn thinking continuity (Anthropic).
    /// `provider_details` carries provider-native replay metadata, such as
    /// OpenRouter reasoning_details. Callers that build ContentBlock::Thinking
    /// from stream events must store these fields alongside accumulated text.
    ThinkingEnd {
        signature: Option<String>,
        provider_details: Option<Value>,
    },
    ToolUseStart { id: String, name: String },
    ToolUseArgsChunk { id: String, delta: String },
    /// Signals all arguments for this tool use have been received.
    /// The accumulated JSON args are now complete and can be parsed.
    /// Anthropic: maps to `content_block_stop` for tool_use blocks.
    /// OpenAI-compat: emitted for each accumulated tool use when
    /// `finish_reason: "tool_calls"` arrives.
    ToolUseEnd { id: String },
    Done { usage: TokenUsage },
}
```

Renamed from `ModelStreamChunk` to `StreamEvent` for clarity. Core provides a type alias `pub type ModelStreamChunk = StreamEvent;` for backward compatibility during migration.

### Runtime compatibility contract

The runtime does not depend on how a provider internally implements streaming or non-streaming calls. It only depends on the provider crate's public contract:

- `stream_chat()` emits provider-neutral `StreamEvent`s in the same order the provider exposes them.
- The returned `ModelResponse` contains the complete assistant message for the turn, including text, tool uses, usage, stop reason, and any reasoning metadata needed for continuation.
- `chat()` is semantically equivalent to `stream_chat()` without event delivery.
- Both helpers call the adapter's underlying `ModelAdapter::complete()` implementation.
- `capabilities()` exposes provider/model differences in normalized Orchest terms, not provider API parameter names.
- Tool-use continuation is provider-correct: adapters serialize prior assistant `ToolUse`, `ToolResult`, and required `Thinking` metadata back into the provider's request shape.

Usage contract:

- `StreamEvent::Done { usage }` and `ModelResponse.usage` must match when the provider reports final usage.
- If a provider stream is interrupted before final usage arrives, the adapter returns `ModelError { code: Some("stream_interrupted"), ... }` rather than emitting a misleading `Done` with default usage.
- If the provider successfully completes but omits usage, `TokenUsage::default()` is allowed and `ModelResponse.option_adjustments` must include `OptionAdjustment { option: "usage", reason: "usage_not_reported", ... }`.

Provider-specific branching stays inside adapters. Core must continue to operate only on `Message`, `ContentBlock`, `ModelResponse`, `ToolDef`, and `StreamEvent`.

### Response types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    /// Non-empty when the adapter coerced, ignored, or translated a request
    /// option in a way the caller may care about.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub option_adjustments: Vec<OptionAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Tokens spent on hidden or visible reasoning/thinking when reported.
    /// Zero when the provider does not report this separately.
    pub reasoning_tokens: u64,
    /// Tokens read from prompt cache (cost reduced). Reported by all providers
    /// under different names — adapters normalize to this field.
    /// Anthropic: `cache_read_input_tokens`, DeepSeek: `prompt_cache_hit_tokens`,
    /// OpenAI: automatic (in cached_tokens), OpenRouter: `prompt_tokens_details.cached_tokens`.
    pub cache_read_tokens: u64,
    /// Tokens written to prompt cache (may cost more on first write).
    /// Anthropic: `cache_creation_input_tokens`, DeepSeek: implicit,
    /// OpenAI: free. Zero when provider doesn't report separately.
    pub cache_write_tokens: u64,
    /// Provider-specific usage details that don't fit the typed fields.
    /// Examples: audio tokens, reasoning tokens breakdown, etc.
    /// Keys are provider-defined (e.g., "reasoning_tokens", "audio_input_tokens").
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub details: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ContentFilter,
    Refusal,
    ContextWindowExceeded,
    Pause,
    Interrupted,
    Other(String),
}
```

Stop reason mapping is part of the provider normalization contract:

- Anthropic `end_turn` → `EndTurn`, `tool_use` → `ToolUse`, `max_tokens` → `MaxTokens`, `stop_sequence` → `StopSequence`, `pause_turn` / `compaction` → `Pause`, `refusal` → `Refusal`, `model_context_window_exceeded` → `ContextWindowExceeded`.
- OpenAI Chat Completions `stop` → `EndTurn`, `tool_calls` / deprecated `function_call` → `ToolUse`, `length` → `MaxTokens`, `content_filter` → `ContentFilter`.
- DeepSeek `stop` → `EndTurn`, `tool_calls` → `ToolUse`, `length` → `MaxTokens`, `content_filter` → `ContentFilter`, `insufficient_system_resource` → `Interrupted`.
- Unknown provider stop reasons map to `Other(raw_reason)` rather than being collapsed into `EndTurn`.

### Error type

```rust
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub message: String,
    pub code: Option<String>,
    pub provider: Option<String>,
    pub status: Option<u16>,
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub upstream_body: Option<Value>,
}

impl ModelError {
    pub fn internal(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: Some(code.into()),
            provider: None,
            status: None,
            upstream_code: None,
            upstream_message: None,
            upstream_body: None,
        }
    }
}
```

Provider error handling is part of the public contract:

- Adapters must never convert an upstream provider error into a successful empty response.
- HTTP status codes, provider error codes, provider messages, and structured error bodies must be preserved when available.
- Stable Orchest `code` values are still required for programmatic handling, e.g. `rate_limited`, `authentication_failed`, `invalid_request`, `invalid_tool_arguments`, `provider_unavailable`, `stream_interrupted`, `unsupported_reasoning_level`.
- If an SSE stream contains malformed JSON or terminates unexpectedly, return `ModelError` with the raw chunk or parser context in `upstream_body` / `upstream_message` where possible.
- Redaction is allowed only for secrets such as API keys in headers. Provider response bodies should otherwise be preserved for debugging.

### Model spec

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub context_window_size: Option<u64>,
}
```

`ModelSpec` is construction/configuration input, not capability metadata. Runtime behavior must use `ModelAdapter::capabilities()`, whose source is tracked through `CapabilitySource`.

## Observability impact

SDK-wide logging, metrics, token attribution, and error observability are defined in [docs/polaris/observability.md](../../polaris/observability.md). This proposal does not redefine those rules.

Provider extraction adds the provider-side implementation of that contract:

- `agent-runtime-providers` emits `model.complete` and `provider.request` spans.
- Provider adapters record the model metrics defined in Polaris, including request duration, stream first-token latency, stream duration, token counters, and missing-usage counters.
- Provider adapters derive `model_family` from normalized model references, including routed OpenRouter model paths when recognizable.
- Provider adapters use normalized `TokenUsage` as the only source for token metrics and for the final `ModelResponse.usage`.
- Provider adapters preserve upstream errors in `ModelError` while keeping provider raw bodies out of default tracing output.

Core migration adds the core-side implementation of the same Polaris contract:

- `agent.run`, `tool.execute`, `mcp.request`, `skill.load`, and `subagent.run` spans.
- Tool, MCP, run, and budget metrics.
- BudgetGuard accounting from the same `ModelResponse.usage` that providers return.

## File layout

```
crates/agent-runtime-providers/
  Cargo.toml
  src/
    lib.rs              -- pub use, create_adapter factory
    types.rs            -- all public types above
    telemetry.rs        -- tracing span helpers + metric name constants
    sse.rs              -- shared SSE line parser for OpenAI-compat protocols
    anthropic.rs        -- AnthropicAdapter + AnthropicConfig
    openai.rs           -- OpenAiAdapter + OpenAiConfig
    deepseek.rs         -- DeepSeekAdapter + DeepSeekConfig
    openrouter.rs       -- OpenRouterAdapter + OpenRouterConfig
```

No feature flags. All four adapters are compiled unconditionally. The current providers are all pure reqwest/SSE with no heavy dependencies. Feature flags will be introduced when a heavyweight provider (e.g., AWS Bedrock with `aws-sdk-bedrockruntime`) is added.

## Cargo.toml

```toml
[package]
name = "agent-runtime-providers"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["sync"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
reqwest = { version = "0.12", features = ["json", "stream"] }
futures-util = "0.3"
thiserror = "2"
tracing = "0.1"
metrics = "0.24"

[dev-dependencies]
tracing-subscriber = { version = "0.3", features = ["fmt", "registry"] }
metrics-util = "0.20"
```

No dependency on any workspace-internal crate.

## Capability metadata policy

`ModelAdapter::capabilities()` is required for strict/coerce behavior, but providers differ in how much model metadata they expose. Adapters populate capabilities in this order:

1. Provider metadata endpoint or SDK model listing, when available and precise enough.
2. Orchest-maintained static table for known models, versioned with the providers crate.
3. Conservative fallback for unknown models: advertise only baseline chat, streaming, and tool-use behavior that is known for the provider family; mark `source: CapabilitySource::Assumed`.

Strict compatibility checks may only rely on `ProviderMetadata` or `Static` capability entries. If a required field is `Assumed`, `CompatibilityPolicy::Strict` returns `ModelError { code: Some("unknown_model_capability"), ... }` instead of guessing.

## Adapter designs

### Anthropic

Migrated from `core/src/model/anthropic.rs`. Uses the Anthropic Messages API with SSE streaming.

```rust
pub struct AnthropicAdapter { ... }

pub struct AnthropicConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,     // fallback: ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN
    pub api_url: Option<String>,     // fallback: ANTHROPIC_API_URL; default: https://api.anthropic.com/v1/messages
}
```

Protocol: `POST /v1/messages` with `x-api-key` header and `anthropic-version: 2023-06-01`. The adapter includes `anthropic-beta` values only when a selected feature requires a beta header for the target API version, such as extended cache TTL.

SSE event types: `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`.

Thinking support: maps `content_block_start` with `type: "thinking"` and `thinking_delta` to `StreamEvent::ThinkingStart/Thinking/ThinkingEnd` and `ContentBlock::Thinking`.

ThinkingLevel mapping: Anthropic direct Messages API supports three thinking modes: `disabled`, `enabled`, and `adaptive`. The adapter selects between `adaptive` and `enabled` based on the configured model's capabilities:

- **`adaptive`** (newer models such as Opus 4.6+ and Sonnet 4.6+): model decides whether to think per turn. Thinking depth is controlled by the request's `output_config.effort`, not by `thinking.budget_tokens`. This is the preferred mode when available.
- **`enabled`** (older models fallback): forces thinking on every turn. Thinking depth is controlled by `thinking.budget_tokens`.

ThinkingLevel maps differently per mode:

| ThinkingLevel | `adaptive` (`output_config.effort`) | `enabled` (`thinking.budget_tokens`) |
|---------------|--------------------------------------|--------------------------------------|
| Off | `thinking.type: "disabled"` | `thinking.type: "disabled"` |
| Minimal | `"low"` | 1024 |
| Low | `"low"` | 4096 |
| Medium | `"medium"` | 10240 |
| High | `"high"` | 32768 |
| XHigh | `"xhigh"` | max allowed |

`RequestOptions::thinking_budget_tokens` only applies in `enabled` mode — it overrides the level's default budget. In `adaptive` mode this field is ignored because direct Anthropic adaptive thinking uses `output_config.effort`; when `CompatibilityPolicy::Coerce` is active, this produces an `OptionAdjustment { option: "thinking_budget_tokens", reason: "unsupported_in_adaptive_thinking", ... }`.

Output control: `include_thinking` maps to direct API `thinking.display` — `true`→`"summarized"`, `false`→`"omitted"`. With `"omitted"`, thinking text is redacted while continuity metadata/signatures are still returned and must be preserved.

Prompt caching: Anthropic supports both top-level and block-level `cache_control`:

- Top-level `cache_control` automatically applies a cache marker to the last cacheable block in the request.
- Block-level `cache_control` creates explicit cache breakpoints on supported tool, system, message, tool-use, and tool-result blocks.
- Thinking blocks cannot be explicitly marked with `cache_control`, but prior thinking blocks can be cached alongside other request content when round-tripped for continuation.

For the normalized `CachePolicy`, the adapter uses top-level `cache_control` because the public API does not expose per-block cache placement yet. `Auto` sends `{ type: "ephemeral" }`; `Long` sends `{ type: "ephemeral", ttl: "1h" }`; `None` omits cache control. If Anthropic requires a beta header for the selected cache feature, the adapter adds the matching `anthropic-beta` value. Usage fields `cache_read_input_tokens`, `cache_creation_input_tokens`, and `cache_creation.ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens` map to `TokenUsage`.

### OpenAI

Migrated from `core/src/model/openai.rs`. Uses the OpenAI Chat Completions API with SSE streaming.

```rust
pub struct OpenAiAdapter { ... }

pub struct OpenAiConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,     // fallback: OPENAI_API_KEY
    pub api_url: Option<String>,     // fallback: OPENAI_API_URL, OPENAI_BASE_URL; default: https://api.openai.com/v1/chat/completions
}
```

Protocol: `POST /v1/chat/completions` with `Authorization: Bearer` header, `stream: true`, `stream_options: { include_usage: true }`.

SSE format: `data: {...}` lines with `choices[0].delta` containing `content`, `tool_calls`, or `role`. `data: [DONE]` signals end.

ThinkingLevel mapping: maps to OpenAI's `reasoning_effort` parameter. Off→`"none"` when the configured model supports it; Minimal→`"minimal"` when supported, otherwise `"low"`; Low→`"low"`; Medium→`"medium"`; High→`"high"`; XHigh→`"xhigh"` when supported, otherwise `"high"`. `thinking_budget_tokens` is ignored because OpenAI Chat Completions does not expose explicit reasoning token budgets.

Prompt caching: OpenAI prompt caching is automatic for supported models and reports cached tokens in `usage.prompt_tokens_details.cached_tokens` → `TokenUsage::cache_read_tokens`. `CachePolicy::Auto` uses the provider default. `CachePolicy::Long` maps to `prompt_cache_retention: "24h"` when the configured model supports extended prompt cache retention; otherwise `Coerce` records an `OptionAdjustment` and uses the default retention, while `Strict` returns `ModelError { code: Some("unsupported_cache_retention") }`. `cache_write_tokens` is always 0 because OpenAI does not charge for cache writes.

### DeepSeek

New adapter. OpenAI-compatible protocol with DeepSeek-specific defaults and thinking-mode support.

```rust
pub struct DeepSeekAdapter { ... }

pub struct DeepSeekConfig {
    pub model: String,               // default: "deepseek-chat"
    pub max_tokens: u32,
    pub api_key: Option<String>,     // fallback: DEEPSEEK_API_KEY
    pub api_url: Option<String>,     // default: https://api.deepseek.com
}
```

Protocol: same as OpenAI Chat Completions. Base URL defaults to `https://api.deepseek.com`.

Reasoning support: DeepSeek's `reasoning_content` field in streamed deltas maps to `StreamEvent::Thinking` events and `ContentBlock::Thinking` in the final response.

ThinkingLevel mapping:

- Off → top-level `thinking: { "type": "disabled" }`; omit `reasoning_effort`
- Minimal / Low / Medium / High → top-level `thinking: { "type": "enabled" }` + `reasoning_effort: "high"`
- XHigh → top-level `thinking: { "type": "enabled" }` + `reasoning_effort: "max"`

`thinking_budget_tokens` is ignored. DeepSeek documents `low` and `medium` compatibility as mapping to `high`, and `xhigh` as mapping to `max`; the adapter normalizes Orchest levels directly to the documented `high` / `max` values.

When thinking is enabled, DeepSeek ignores `temperature`, `top_p`, `presence_penalty`, and `frequency_penalty`. The adapter should omit `temperature` and `top_p` from the request in this mode to avoid implying they affect behavior.

DeepSeek does not expose a separate "think but exclude reasoning output" control. If `include_thinking` is `false` while `thinking != Off`, `CompatibilityPolicy::Coerce` disables thinking and records `OptionAdjustment { option: "include_thinking", reason: "output_exclusion_unsupported_disables_reasoning", ... }`; `Strict` returns `ModelError { code: Some("unsupported_reasoning_output_exclusion") }`.

Prompt caching: fully automatic (disk-based KV cache), no configuration needed. `CachePolicy` is ignored. Reports `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens` → mapped to `TokenUsage::cache_read_tokens`. `cache_write_tokens` is 0 (DeepSeek doesn't separate write cost).

Reasoning replay:

- If an assistant turn did not perform a tool call, DeepSeek allows callers to omit `reasoning_content` in later turns; if provided, it is ignored.
- If an assistant turn performed a tool call, DeepSeek requires the assistant message's `reasoning_content` to be passed back in subsequent user interaction turns.

The adapter must therefore preserve DeepSeek thinking content in `ContentBlock::Thinking` and replay it when serializing assistant messages that contain `ToolUse`. It must not blindly strip all thinking blocks from input history.

Internally reuses SSE parsing from `sse.rs`. The adapter handles:
- URL normalization (append `/v1/chat/completions` if needed)
- `DEEPSEEK_API_KEY` env var fallback
- `reasoning_content` delta extraction
- Replaying `reasoning_content` for assistant tool-call turns, omitting it only when the protocol allows omission

Note: `extra_body` is only relevant when using the OpenAI SDK to send DeepSeek-specific fields. This Rust adapter builds HTTP JSON directly and sends `thinking` as a top-level request field.

### OpenRouter

New adapter. OpenAI-compatible protocol with OpenRouter-specific headers and model routing.

```rust
pub struct OpenRouterAdapter { ... }

pub struct OpenRouterConfig {
    pub model: String,               // e.g. "anthropic/claude-sonnet-4"
    pub max_tokens: u32,
    pub api_key: Option<String>,     // fallback: OPENROUTER_API_KEY
    pub api_url: Option<String>,     // default: https://openrouter.ai/api
    pub app_title: Option<String>,   // X-OpenRouter-Title header
    pub site_url: Option<String>,    // HTTP-Referer header
}
```

Protocol: same as OpenAI Chat Completions. Base URL defaults to `https://openrouter.ai/api`.

OpenRouter-specific headers:
- `HTTP-Referer`: set from `site_url` config or `OPENROUTER_SITE_URL` env var
- `X-OpenRouter-Title`: set from `app_title` config or `OPENROUTER_APP_TITLE` env var

Reasoning support: when the underlying model supports reasoning, OpenRouter includes reasoning content in the response. Human-readable reasoning text maps to `StreamEvent::Thinking` and `ContentBlock::Thinking.text`; provider-native `reasoning_details` maps to `ContentBlock::Thinking.provider_details` and `StreamEvent::ThinkingEnd.provider_details`.

ThinkingLevel mapping: Uses OpenRouter's unified `reasoning` object, but sends exactly one of `reasoning.effort` or `reasoning.max_tokens`:

- If `thinking_budget_tokens` is `Some`, send `reasoning.max_tokens` and omit `reasoning.effort`. OpenRouter passes the budget through to providers/models that support explicit reasoning token allocation, or maps it to an effort level for effort-only models.
- Otherwise, send `reasoning.effort`, mapping `ThinkingLevel` directly to `none` / `minimal` / `low` / `medium` / `high` / `xhigh`.
- If the routed model cannot support the selected form, behavior follows `CompatibilityPolicy`.

Prompt caching: OpenRouter uses provider sticky routing to maximize cache hits. Anthropic models via OpenRouter support `cache_control` breakpoints. When `CachePolicy::Auto`, the adapter adds top-level `cache_control` for Anthropic-backed models. Other providers are automatic. Reports `prompt_tokens_details.cached_tokens` → mapped to `TokenUsage::cache_read_tokens`.

Model names pass through unchanged (e.g., `anthropic/claude-sonnet-4`, `openai/gpt-4o`). OpenRouter handles routing internally.

Reasoning replay:

- If OpenRouter returns `reasoning_details`, the adapter must preserve it exactly.
- The adapter must replay `reasoning_details` without reordering, summarizing, filtering, or reconstructing it from text.
- If `include_thinking` is `false`, the adapter sends `reasoning.exclude: true`; the model may still reason internally, but no replay metadata should be expected in the response.

## Shared SSE parsing (sse.rs)

OpenAI, DeepSeek, and OpenRouter all use the same SSE line format. `sse.rs` provides shared utilities:

```rust
/// Parse a raw byte stream into SSE data lines, handling buffering
/// across chunk boundaries. Yields each `data: ...` payload as a
/// serde_json::Value. Skips empty lines, `data: [DONE]`, and
/// comment lines.
pub(crate) async fn parse_openai_sse_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>>,
    tx: Option<&mpsc::Sender<StreamEvent>>,  // None = skip streaming events
    reasoning_field: Option<&str>,  // "reasoning_content" for DeepSeek, "reasoning" for OpenRouter, None for OpenAI
    reasoning_details_field: Option<&str>, // "reasoning_details" for OpenRouter when present
) -> Result<(Vec<ContentBlock>, TokenUsage, StopReason), ModelError>
```

Each adapter calls this with its reasoning field name. The function handles:
- Line buffering across TCP chunk boundaries
- `choices[0].delta.content` text extraction
- `choices[0].delta.tool_calls` accumulation
- `choices[0].delta.{reasoning_field}` thinking extraction (when field name provided)
- provider-native reasoning detail preservation (when `reasoning_details_field` is provided)
- Usage extraction from final chunk
- Stop reason mapping (`tool_calls` -> `StopReason::ToolUse`)

Anthropic uses its own SSE parsing because the Anthropic protocol is structurally different (event types, content blocks, message deltas).

## Factory function

The preferred construction path is a normalized model reference. Provider-specific config structs are escape hatches for tests, custom endpoints, or advanced deployment controls; application code should usually not need to import `AnthropicConfig`, `OpenAiConfig`, etc.

```rust
/// Create a model adapter from a "provider/model" string.
///
/// Supported prefixes:
/// - `anthropic/` -> AnthropicAdapter
/// - `openai/`    -> OpenAiAdapter
/// - `deepseek/`  -> DeepSeekAdapter
/// - `openrouter/` -> OpenRouterAdapter
///
/// The model name after the prefix is passed to the adapter's config.
/// For OpenRouter, the full model path after "openrouter/" is preserved
/// (e.g., "openrouter/anthropic/claude-sonnet-4" -> model = "anthropic/claude-sonnet-4").
///
/// `api_key` overrides the environment variable fallback.
/// `max_tokens` defaults to 4096.
pub fn create_adapter(
    model: &str,
    api_key: Option<String>,
) -> Result<Box<dyn ModelAdapter>, ModelError>
```

Implementation routes on the first `/`-delimited segment:

```rust
pub fn create_adapter(model: &str, api_key: Option<String>) -> Result<Box<dyn ModelAdapter>, ModelError> {
    let (provider, model_name) = model.split_once('/')
        .ok_or_else(|| ModelError::internal(
            format!("model string must be 'provider/model', got '{model}'"),
            "invalid_model",
        ))?;

    match provider {
        "anthropic" => Ok(Box::new(AnthropicAdapter::from_config(AnthropicConfig {
            model: model_name.to_string(),
            max_tokens: 4096,
            api_key,
            api_url: None,
        })?)),
        "openai" => Ok(Box::new(OpenAiAdapter::from_config(OpenAiConfig {
            model: model_name.to_string(),
            max_tokens: 4096,
            api_key,
            api_url: None,
        })?)),
        "deepseek" => Ok(Box::new(DeepSeekAdapter::from_config(DeepSeekConfig {
            model: model_name.to_string(),
            max_tokens: 4096,
            api_key,
            api_url: None,
        })?)),
        "openrouter" => {
            // For openrouter, everything after "openrouter/" is the model
            // e.g. "openrouter/anthropic/claude-sonnet-4" -> "anthropic/claude-sonnet-4"
            let full_model = &model["openrouter/".len()..];
            Ok(Box::new(OpenRouterAdapter::from_config(OpenRouterConfig {
                model: full_model.to_string(),
                max_tokens: 4096,
                api_key,
                api_url: None,
                app_title: None,
                site_url: None,
            })?))
        }
        _ => Err(ModelError::internal(
            format!("unknown provider '{provider}'. Supported: anthropic, openai, deepseek, openrouter"),
            "unknown_provider",
        )),
    }
}
```

## Core migration

### Changes to agent-runtime-core

1. **`Cargo.toml`**: add:
   ```toml
   [dependencies]
   agent-runtime-providers = { path = "../agent-runtime-providers" }
   tracing = "0.1"
   metrics = "0.24"

   [dev-dependencies]
   tracing-subscriber = { version = "0.3", features = ["fmt", "registry"] }
   metrics-util = "0.20"
   ```

2. **`src/model/mod.rs`**: replace type definitions with re-exports:
   ```rust
   pub use agent_runtime_providers::{
       ModelAdapter, Message, Role, ContentBlock,
       ToolDef, JsonSchema, ModelResponse, ModelError,
       StreamEvent, RequestOptions,
       ThinkingLevel, CachePolicy, CompatibilityPolicy,
       CapabilitySource, ModelCapabilities, ReasoningCapability, CacheCapability,
       OptionAdjustment,
       TokenUsage, StopReason, ModelSpec,
       AnthropicAdapter, AnthropicConfig,
       OpenAiAdapter, OpenAiConfig,
       DeepSeekAdapter, DeepSeekConfig,
       OpenRouterAdapter, OpenRouterConfig,
       create_adapter, stream_chat, chat,
   };

   /// Backward-compatible alias.
   pub type ModelStreamChunk = StreamEvent;
   ```

3. **`src/model/anthropic.rs`** and **`src/model/openai.rs`**: delete. Code moves to providers.

4. **`src/tool/mod.rs`**: `ToolDef` and `JsonSchema` are now re-exported from providers via `model/mod.rs`. Update the `use` path. The `Tool` trait, `ToolMetadata`, `ToolOutput`, and other tool-specific types remain in core.

5. **`src/run.rs`**: update import paths. `ModelStreamChunk` usages either switch to `StreamEvent` or use the type alias. No logic changes.

6. **`src/events.rs`**: `ModelStreamChunk` in the `RuntimeEvent::ModelStreamChunk` variant — keep the variant name but use the type alias. Or rename the variant to `ModelStreamEvent` with a `delta: StreamEvent` field. Either works; variant rename is cleaner but a larger diff.

7. **`src/tool/mcp.rs`**: if it uses `ToolDef`, update the import path.

8. **`src/telemetry.rs`**: add core-owned tracing span helpers and metric name constants for run loop, tool execution, MCP transport, Skill loading, sub-agent runs, and budget accounting. These helpers must use the SDK-wide names from [Polaris observability](../../polaris/observability.md) and must not install a subscriber or exporter.

9. **`src/run.rs` / `src/tool/*` / `src/skill/*` / `src/budget.rs`**: wrap major operations in the canonical spans and record metrics from normalized runtime outcomes:
   - `agent.run` spans around run-loop execution
   - `tool.execute` spans around direct, MCP-backed, Skill-backed, and built-in tool execution
   - `mcp.request` spans around MCP transport calls
   - `skill.load` spans around Skill discovery/load
   - budget metrics from the same usage values consumed by `BudgetGuard`

### Changes to SDKs

Minimal. SDKs access types through core re-exports:
```rust
// Before:
use agent_runtime_core::model::anthropic::AnthropicAdapter;
// After (same, because core re-exports):
use agent_runtime_core::model::AnthropicAdapter;
```

The `anthropic` and `openai` submodules in `core::model` are removed, but since core re-exports the types at the `model` level, SDK code that imports from `core::model::anthropic::AnthropicAdapter` needs a one-line path update to `core::model::AnthropicAdapter`.

## Testing

### Provider crate tests

Migrated from core and extended:

| Test | Source | Description |
|------|--------|-------------|
| `anthropic::uses_default_api_url` | migrated | URL fallback |
| `anthropic::uses_custom_api_url` | migrated | Custom URL |
| `anthropic::appends_messages_endpoint` | migrated | URL normalization |
| `anthropic::rejects_empty_api_url` | migrated | Validation |
| `anthropic::stream_thinking_boundaries` | migrated | ThinkingStart/End events |
| `anthropic::stream_rejects_malformed_sse` | migrated | Error handling |
| `anthropic::stream_thinking_to_content_block` | **new** | Thinking -> ContentBlock::Thinking |
| `openai::strips_prefix` | migrated | Model name normalization |
| `openai::stream_text_and_tool_calls` | migrated | Full SSE parse |
| `openai::build_request_body` | migrated | Message serialization |
| `openai::stream_rejects_invalid_tool_args` | migrated | Error handling |
| `deepseek::default_api_url` | **new** | https://api.deepseek.com default |
| `deepseek::reasoning_content_maps_to_thinking` | **new** | DeepSeek thinking support |
| `deepseek::env_var_fallback` | **new** | DEEPSEEK_API_KEY |
| `openrouter::default_api_url` | **new** | https://openrouter.ai/api default |
| `openrouter::sends_custom_headers` | **new** | X-OpenRouter-Title, HTTP-Referer |
| `openrouter::model_passthrough` | **new** | Full model path preserved |
| `openrouter::reasoning_maps_to_thinking` | **new** | OpenRouter thinking support |
| `openrouter::reasoning_details_preserved` | **new** | OpenRouter reasoning_details stored in provider_details and replayed exactly |
| `sse::parse_buffered_chunks` | **new** | Cross-boundary SSE parsing |
| `errors::provider_http_error_preserves_upstream_body` | **new** | Provider status/code/message/body preserved in ModelError |
| `errors::malformed_stream_chunk_is_not_swallowed` | **new** | Malformed SSE JSON returns ModelError with parser context |
| `errors::interrupted_stream_returns_stream_interrupted` | **new** | Missing final stream completion returns stable error, not default successful usage |
| `usage::done_usage_matches_model_response` | **new** | Done usage and ModelResponse usage match on successful streamed calls |
| `usage::missing_usage_reports_adjustment_and_metric` | **new** | Successful response without usage records usage_not_reported and increments usage-missing metric |
| `telemetry::model_complete_span_has_canonical_fields` | **new** | `model.complete` span includes provider/model/streaming/status/error/token fields without prompt content |
| `telemetry::provider_request_metrics_are_low_cardinality` | **new** | Model metrics use provider/model_family/status labels and exclude run_id/raw error text |
| `telemetry::provider_error_metrics_preserve_failure_status` | **new** | Failed provider request increments status=error metrics and does not emit Done |
| `telemetry::first_token_latency_recorded_for_streams` | **new** | Streaming path records first-token histogram when first text/thinking/tool delta arrives |
| `factory::routes_by_provider` | **new** | create_adapter routing |
| `factory::rejects_unknown_provider` | **new** | Error for unsupported provider |
| `factory::openrouter_preserves_full_model` | **new** | "openrouter/anthropic/claude-sonnet-4" |
| `capabilities::all_adapters_report_normalized_capabilities` | **new** | capabilities() uses Orchest concepts, not provider parameter names |
| `capabilities::strict_rejects_assumed_capability` | **new** | Strict mode rejects unknown capability guesses with unknown_model_capability |
| `options::coerce_reports_adjustment` | **new** | CompatibilityPolicy::Coerce records OptionAdjustment for normalized-but-changed options |
| `options::strict_rejects_unsupported_option` | **new** | CompatibilityPolicy::Strict returns stable ModelError for unsupported exact semantics |
| `message_serialization::system_role_maps_per_provider` | **new** | Role::System serializes to Anthropic top-level system and Chat-compatible system/developer messages |
| `message_serialization::tool_results_map_per_provider` | **new** | ToolResult serializes to Anthropic user tool_result blocks and OpenAI-compatible role=tool messages |
| `stop_reason::provider_reasons_are_not_lost` | **new** | refusal/content_filter/context exceeded/interrupted map to stable StopReason variants |
| `anthropic::thinking_level_maps_to_budget` | **new** | ThinkingLevel → budget_tokens mapping |
| `anthropic::thinking_budget_override` | **new** | RequestOptions::thinking_budget_tokens overrides level default |
| `anthropic::thinking_budget_ignored_in_adaptive_reports_adjustment` | **new** | adaptive thinking uses output_config.effort and reports ignored budget override |
| `anthropic::include_thinking_false_maps_to_omitted` | **new** | include_thinking=false → display: "omitted" |
| `anthropic::adaptive_uses_output_config_effort` | **new** | adaptive thinking maps ThinkingLevel to output_config.effort |
| `anthropic::cache_policy_auto_adds_top_level_cache_control` | **new** | CachePolicy::Auto adds top-level cache_control for the last cacheable block |
| `anthropic::cache_policy_long_sets_1h_ttl` | **new** | CachePolicy::Long → ttl: "1h" |
| `anthropic::cache_usage_mapped_to_token_usage` | **new** | cache_read/write_input_tokens → TokenUsage |
| `openai::thinking_level_maps_to_reasoning_effort` | **new** | ThinkingLevel → reasoning_effort |
| `openai::thinking_off_maps_to_none_when_supported` | **new** | ThinkingLevel::Off sends reasoning_effort none for models that support none |
| `openai::cache_policy_long_maps_to_24h_when_supported` | **new** | CachePolicy::Long sends prompt_cache_retention=24h when model supports it |
| `openai::cache_tokens_reported` | **new** | cached_tokens → TokenUsage::cache_read_tokens |
| `deepseek::thinking_off_disables_reasoning` | **new** | ThinkingLevel::Off sends thinking.type disabled and omits reasoning_effort |
| `deepseek::thinking_levels_map_to_high_and_max` | **new** | Minimal/Low/Medium/High -> high; XHigh -> max |
| `deepseek::thinking_is_top_level_not_extra_body` | **new** | Direct HTTP body sends top-level thinking field |
| `deepseek::omits_sampling_when_thinking_enabled` | **new** | temperature/top_p omitted when thinking is enabled |
| `deepseek::include_thinking_false_reports_or_errors` | **new** | output exclusion unsupported: Coerce adjustment or Strict error |
| `deepseek::replays_reasoning_for_tool_call_turns` | **new** | reasoning_content replayed when assistant message contains tool_calls |
| `deepseek::cache_hit_tokens_reported` | **new** | prompt_cache_hit_tokens → TokenUsage |
| `openrouter::reasoning_object_from_thinking_level` | **new** | ThinkingLevel → reasoning.effort |
| `openrouter::reasoning_effort_and_max_tokens_are_exclusive` | **new** | explicit budget sends max_tokens and omits effort |
| `runtime_contract::stream_chat_and_chat_are_semantically_equivalent` | **new** | chat() returns the same final ModelResponse as stream_chat() without event delivery |
| `runtime_contract::helpers_use_model_adapter_complete` | **new** | chat() and stream_chat() call the same underlying ModelAdapter::complete path |
| `runtime_contract::stream_chat_requires_receiver_drain` | **new** | stream_chat docs/tests cover bounded-channel backpressure behavior |
| `runtime_contract::core_never_branches_on_provider` | **new** | Core tests use only ModelAdapter, Message, ContentBlock, ToolDef, StreamEvent |
| `complete::tx_none_skips_events` | **new** | ModelAdapter::complete(tx=None) returns response without streaming |
| `stream::tool_use_end_emitted` | **new** | ToolUseEnd emitted after tool use args are complete |
| `stream::parallel_tool_uses_end_each` | **new** | Each parallel tool use gets its own ToolUseEnd |
| `anthropic::temperature_forwarded` | **new** | RequestOptions::temperature → API `temperature` |
| `openai::temperature_forwarded` | **new** | RequestOptions::temperature → API `temperature` |
| `anthropic::adaptive_vs_enabled_mode` | **new** | Newer models use adaptive, older use enabled |

All SSE tests use the existing `serve_sse_once` pattern (bind to `127.0.0.1:0`, serve one HTTP response with SSE body).

### Provider API references

Adapter mappings in this design are based on provider documentation current as of 2026-05-24:

- DeepSeek Thinking Mode: https://api-docs.deepseek.com/guides/thinking_mode
- Anthropic Messages API: https://docs.claude.com/en/api/messages
- Anthropic Prompt Caching: https://docs.claude.com/en/docs/build-with-claude/prompt-caching
- OpenRouter Reasoning Tokens: https://openrouter.ai/docs/guides/best-practices/reasoning-tokens

### Core test changes

- Model-specific tests (SSE parsing, URL normalization) are removed from core — now covered by providers crate.
- Core retains `MockModelProvider`-based tests for run loop behavior. `MockModelProvider` stays in core (it implements `ModelAdapter` from providers).
- Integration tests (`e2e_validation.rs`, `v03_runtime.rs`) continue to work unchanged since they use `MockModelProvider`.
- Add core observability tests with an in-memory tracing subscriber and metrics recorder:
  - `telemetry::agent_run_span_has_run_fields`
  - `telemetry::tool_execute_span_excludes_tool_payload`
  - `telemetry::budget_metrics_use_model_response_usage`
  - `telemetry::metrics_do_not_include_high_cardinality_labels`

## Standalone usage example

```rust
use agent_runtime_providers::{
    create_adapter, stream_chat, chat,
    Message, ContentBlock, Role, StreamEvent,
    RequestOptions, ThinkingLevel, CachePolicy,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = create_adapter("anthropic/claude-sonnet-4-20250514", None)?;

    let messages = vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text("You are helpful.".into())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Explain Rust's ownership model.".into())],
        },
    ];

    // -- Streaming example --
    let options = RequestOptions {
        thinking: ThinkingLevel::High,
        cache_policy: CachePolicy::Auto,
        ..Default::default()
    };

    let (fut, mut rx) = stream_chat(adapter.as_ref(), &messages, &[], &options);
    let handle = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Text { delta } => print!("{delta}"),
                StreamEvent::ThinkingStart => print!("[thinking] "),
                StreamEvent::Thinking { delta } => print!("{delta}"),
                StreamEvent::ThinkingEnd { .. } => println!(" [/thinking]"),
                StreamEvent::Done { usage } => {
                    println!("\n(in:{} out:{} cache_read:{} cache_write:{})",
                        usage.input_tokens, usage.output_tokens,
                        usage.cache_read_tokens, usage.cache_write_tokens);
                }
                _ => {}
            }
        }
    });

    let response = fut.await?;
    handle.await?;
    println!("Stop reason: {:?}", response.stop_reason);
    if !response.option_adjustments.is_empty() {
        println!("Option adjustments: {:?}", response.option_adjustments);
    }

    // -- Non-streaming example (same interface, just use chat()) --
    let response = chat(adapter.as_ref(), &messages, &[], &options).await?;
    println!("Got {} content blocks", response.content.len());

    // -- Think deeply but hide thinking from output (lower latency) --
    let _fast = RequestOptions {
        thinking: ThinkingLevel::High,   // 怎么思考: deep
        include_thinking: false,          // 输出不包含思考
        ..Default::default()
    };

    // -- Advanced: explicit reasoning budget + long cache when supported --
    let _advanced = RequestOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: Some(16384),
        include_thinking: true,
        cache_policy: CachePolicy::Long,  // provider-specific extended retention
        ..Default::default()
    };

    Ok(())
}
```

## Migration sequence

To minimize breakage, execute in this order:

1. Create `crates/agent-runtime-providers/` with `Cargo.toml` and `src/types.rs`
2. Move `anthropic.rs` and `openai.rs` from core, updating imports to use local `types`
3. Add `deepseek.rs` and `openrouter.rs`
4. Add `sse.rs` with shared OpenAI-compat parsing, refactor openai/deepseek/openrouter to use it
5. Add `lib.rs` with public exports and `create_adapter`
6. Add providers crate to workspace `Cargo.toml`
7. Update core: add providers dependency, replace `model/mod.rs` with re-exports, delete `model/anthropic.rs` and `model/openai.rs`
8. Update core import paths in `run.rs`, `events.rs`, `tool/mod.rs`
9. Add SDK-wide observability helpers and instrumentation in providers/core
10. Update SDK import paths in `agent-runtime-py` and `agent-runtime-node`
11. Run `cargo test --workspace`, fix any remaining path issues
12. Run `cargo clippy --workspace -- -D warnings` and `cargo fmt --check`

## Future extensions

When adding heavyweight providers (AWS Bedrock, Google Vertex AI), introduce feature flags at that point:

```toml
[features]
default = []  # lightweight providers always compiled
bedrock = ["dep:aws-sdk-bedrockruntime", "dep:aws-config"]
vertex = ["dep:google-cloud-auth"]
```

This avoids pulling heavy SDK dependencies for users who only need reqwest-based providers.
