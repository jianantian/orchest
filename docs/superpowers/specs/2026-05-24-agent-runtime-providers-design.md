# agent-runtime-providers crate design

> Date: 2026-05-24
> Status: Approved

## Goal

Extract LLM provider logic from `agent-runtime-core` into a standalone `agent-runtime-providers` crate. The crate must be independently usable — `cargo add agent-runtime-providers` gives users a complete LLM calling library without pulling in the agent runtime (run loop, tools, skills, MCP, budget, etc.).

Initial providers: Anthropic, OpenAI, DeepSeek, OpenRouter.

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
        options: &StreamOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError>;
}
```

Both streaming and non-streaming go through `complete()`. Convenience free functions are provided:

```rust
/// Streaming helper — creates a channel and returns (response_future, receiver).
pub fn stream(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &StreamOptions,
) -> (impl Future<Output = Result<ModelResponse, ModelError>> + '_, mpsc::Receiver<StreamEvent>) {
    let (tx, rx) = mpsc::channel(64);
    let fut = adapter.complete(messages, tools, options, Some(tx));
    (fut, rx)
}

/// Non-streaming helper — discards events, returns only the final response.
pub async fn call(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &StreamOptions,
) -> Result<ModelResponse, ModelError> {
    adapter.complete(messages, tools, options, None).await
}
```

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
        text: String,
        /// Opaque signature for multi-turn thinking continuity.
        /// Anthropic requires thinking blocks to be passed back unchanged
        /// with their signature intact. Other providers may use this for
        /// similar purposes (e.g., OpenAI encrypted_content).
        /// Must be preserved exactly when replaying assistant messages.
        signature: Option<String>,
    },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: Value },
}
```

`ContentBlock::Thinking` is new compared to the current core types. It persists thinking content in the message history rather than only surfacing it through streaming events. The `signature` field is critical for Anthropic multi-turn conversations — thinking blocks must be passed back to the API with their signature intact to maintain reasoning continuity. This enables:

- Context compaction that preserves reasoning traces
- Sub-agent message forwarding that includes thinking
- Session persistence of thinking blocks
- Multi-turn thinking continuity (via signature preservation)

### Thinking, output, and caching configuration

Thinking has three independent concerns:

1. **是否思考 (whether to think)** — binary on/off, controlled by `ThinkingLevel::Off` vs any other level
2. **怎么思考 (how to think)** — depth/effort, controlled by `ThinkingLevel` granularity + optional `budget_tokens`
3. **输出是否包含思考 (whether output includes thinking)** — independent of #1 and #2, controlled by `include_thinking`

These are orthogonal: a model can think deeply but exclude thinking from the output (saves streaming latency), or think minimally but still include it (for transparency).

```rust
/// Controls thinking depth — combines "whether" and "how deep".
/// Each adapter maps these levels to provider-specific parameters.
/// Not all providers support all levels — adapters clamp to the
/// nearest supported value.
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
    /// Long-lived cache (Anthropic: 1-hour TTL; others: best-effort).
    Long,
}
```

### StreamOptions

```rust
/// Per-request options passed to `ModelAdapter::complete()`.
/// The same options struct is used for both streaming and non-streaming calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    // --- 是否思考 + 怎么思考 (thinking behavior) ---

    /// Thinking depth. Default: `Medium`.
    pub thinking: ThinkingLevel,

    /// Advanced: override the provider's default token budget for this
    /// thinking level. Most models do not support explicit budget control
    /// — only Anthropic honors this. Ignored when `thinking` is `Off`.
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
    /// When `false`: the model still thinks internally (and is billed
    /// for thinking tokens), but thinking is excluded from output.
    /// Reduces streaming latency (faster time-to-first-text-token).
    ///
    /// Provider mapping:
    /// - Anthropic: maps to `thinking.display` ("summarized" vs "omitted")
    /// - OpenAI: reasoning tokens are never exposed (always false effectively)
    /// - DeepSeek: `false` strips reasoning_content from response
    /// - OpenRouter: maps to `reasoning.exclude`
    pub include_thinking: bool,

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

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            thinking: ThinkingLevel::default(),
            thinking_budget_tokens: None,
            include_thinking: true,
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
| **Anthropic** | `thinking.type` + `thinking.budget_tokens` | Off→`disabled`, Minimal-XHigh→`adaptive` (newer models) or `enabled` (older), with budget_tokens mapped per level. See adapter section for details. | ✅ Supported. Overrides level default when set. |
| **OpenAI** | `reasoning_effort` | Off→omit param, Minimal/Low→"low", Medium→"medium", High/XHigh→"high" | ❌ Ignored |
| **DeepSeek** | Model-level (deepseek-reasoner) | Off→no reasoning, all others→enable reasoning. Binary on/off only. | ❌ Ignored |
| **OpenRouter** | `reasoning: { effort, max_tokens }` | Passes `effort` string directly (none/minimal/low/medium/high/xhigh). | ✅ Forwarded as `reasoning.max_tokens` when underlying model supports it. |

### include_thinking provider mapping

`include_thinking` controls dimension #3 independently:

| Provider   | `true` (default) | `false` |
|------------|-------------------|---------|
| **Anthropic** | `thinking.display: "summarized"` — thinking streamed and in response | `thinking.display: "omitted"` — no thinking in stream, signature-only block in response |
| **OpenAI** | No effect — OpenAI never exposes reasoning tokens to callers | No effect |
| **DeepSeek** | `reasoning_content` included in response and streamed | `reasoning_content` stripped from response |
| **OpenRouter** | `reasoning.exclude: false` — thinking included | `reasoning.exclude: true` — thinking excluded from response |

When a provider doesn't support a requested level, the adapter clamps silently to the nearest supported value (e.g., DeepSeek only has on/off, so Minimal through XHigh all map to "on").

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

### Streaming events

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    Text { delta: String },
    ThinkingStart,
    Thinking { delta: String },
    /// Signals the end of a thinking block. `signature` carries the
    /// opaque token needed for multi-turn thinking continuity (Anthropic).
    /// Callers that build ContentBlock::Thinking from stream events
    /// should store this signature alongside the accumulated text.
    ThinkingEnd { signature: Option<String> },
    ToolCallStart { id: String, name: String },
    ToolCallArgsChunk { id: String, delta: String },
    /// Signals all arguments for this tool call have been received.
    /// The accumulated JSON args are now complete and can be parsed.
    /// Anthropic: maps to `content_block_stop` for tool_use blocks.
    /// OpenAI-compat: emitted for each accumulated tool call when
    /// `finish_reason: "tool_calls"` arrives.
    ToolCallEnd { id: String },
    Done { usage: TokenUsage },
}
```

Renamed from `ModelStreamChunk` to `StreamEvent` for clarity. Core provides a type alias `pub type ModelStreamChunk = StreamEvent;` for backward compatibility during migration.

### Response types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
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
}
```

### Error type

```rust
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub message: String,
    pub code: Option<String>,
}
```

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

## File layout

```
crates/agent-runtime-providers/
  Cargo.toml
  src/
    lib.rs              -- pub use, create_adapter factory
    types.rs            -- all public types above
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
```

No dependency on any workspace-internal crate.

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

Protocol: `POST /v1/messages` with `x-api-key` header and `anthropic-version: 2023-06-01`.

SSE event types: `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`.

Thinking support: maps `content_block_start` with `type: "thinking"` and `thinking_delta` to `StreamEvent::ThinkingStart/Thinking/ThinkingEnd` and `ContentBlock::Thinking`.

ThinkingLevel mapping: Two Anthropic thinking modes, selected by adapter based on model:

- **`adaptive`** (Opus 4.7+, Sonnet 4.6+): model decides whether to think per turn. `budget_tokens` is the ceiling — model may use less or skip entirely if the query is simple. This is preferred.
- **`enabled`** (older models fallback): forces thinking on every turn. `budget_tokens` is a fixed budget.

Both modes use `budget_tokens`, mapped from ThinkingLevel: Off→`thinking.type: "disabled"`, Minimal→1024, Low→4096, Medium→10240, High→32768, XHigh→max allowed. `StreamOptions::thinking_budget_tokens` overrides the level's default budget.

Output control: `include_thinking` maps to `thinking.display` — `true`→"summarized", `false`→"omitted".

Prompt caching: Anthropic requires explicit `cache_control` markers. When `CachePolicy::Auto` or `Long`, the adapter adds `cache_control: { type: "ephemeral" }` to the top-level request (automatic caching mode). `Long` adds `ttl: "1h"`. `None` omits cache_control entirely. Reports `cache_read_input_tokens` and `cache_creation_input_tokens` in usage → mapped to `TokenUsage::cache_read_tokens` / `cache_write_tokens`.

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

ThinkingLevel mapping: maps to OpenAI's `reasoning_effort` parameter. Off→omit parameter, Minimal/Low→"low", Medium→"medium", High/XHigh→"high". `budget_tokens` is ignored (OpenAI does not support it).

Prompt caching: fully automatic (≥1024 tokens), no configuration needed. `CachePolicy` is ignored. OpenAI reports cached tokens in usage → mapped to `TokenUsage::cache_read_tokens`. `cache_write_tokens` is always 0 (OpenAI doesn't charge for cache writes).

### DeepSeek

New adapter. OpenAI-compatible protocol with DeepSeek-specific defaults and reasoning support.

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

ThinkingLevel mapping: DeepSeek only supports on/off for reasoning. Off→disable reasoning, Minimal through XHigh→enable reasoning. `budget_tokens` is ignored.

Prompt caching: fully automatic (disk-based KV cache), no configuration needed. `CachePolicy` is ignored. Reports `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens` → mapped to `TokenUsage::cache_read_tokens`. `cache_write_tokens` is 0 (DeepSeek doesn't separate write cost).

Note: `reasoning_content` must NOT be included in input messages for subsequent turns (API returns 400). The adapter strips thinking content from assistant messages when building the request body.

Internally reuses SSE parsing from `sse.rs`. The adapter handles:
- URL normalization (append `/v1/chat/completions` if needed)
- `DEEPSEEK_API_KEY` env var fallback
- `reasoning_content` delta extraction
- Stripping `reasoning_content` from input assistant messages

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

Reasoning support: when the underlying model supports reasoning, OpenRouter includes `reasoning` content in the response. Mapped to `StreamEvent::Thinking` and `ContentBlock::Thinking`.

ThinkingLevel mapping: Uses OpenRouter's unified `reasoning` object: `{ effort: "none"|"minimal"|"low"|"medium"|"high"|"xhigh", max_tokens: ... }`. ThinkingLevel maps directly to the `effort` string. `thinking_budget_tokens` is forwarded as `reasoning.max_tokens` (OpenRouter passes it to the underlying provider when supported).

Prompt caching: OpenRouter uses provider sticky routing to maximize cache hits. Anthropic models via OpenRouter support `cache_control` breakpoints. When `CachePolicy::Auto`, the adapter adds top-level `cache_control` for Anthropic-backed models. Other providers are automatic. Reports `prompt_tokens_details.cached_tokens` → mapped to `TokenUsage::cache_read_tokens`.

Model names pass through unchanged (e.g., `anthropic/claude-sonnet-4`, `openai/gpt-4o`). OpenRouter handles routing internally.

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
) -> Result<(Vec<ContentBlock>, TokenUsage, StopReason), ModelError>
```

Each adapter calls this with its reasoning field name. The function handles:
- Line buffering across TCP chunk boundaries
- `choices[0].delta.content` text extraction
- `choices[0].delta.tool_calls` accumulation
- `choices[0].delta.{reasoning_field}` thinking extraction (when field name provided)
- Usage extraction from final chunk
- Stop reason mapping (`tool_calls` -> `StopReason::ToolUse`)

Anthropic uses its own SSE parsing because the Anthropic protocol is structurally different (event types, content blocks, message deltas).

## Factory function

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
        .ok_or_else(|| ModelError {
            message: format!("model string must be 'provider/model', got '{model}'"),
            code: Some("invalid_model".into()),
        })?;

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
        _ => Err(ModelError {
            message: format!("unknown provider '{provider}'. Supported: anthropic, openai, deepseek, openrouter"),
            code: Some("unknown_provider".into()),
        }),
    }
}
```

## Core migration

### Changes to agent-runtime-core

1. **`Cargo.toml`**: add `agent-runtime-providers = { path = "../agent-runtime-providers" }`

2. **`src/model/mod.rs`**: replace type definitions with re-exports:
   ```rust
   pub use agent_runtime_providers::{
       ModelAdapter, Message, Role, ContentBlock,
       ToolDef, JsonSchema, ModelResponse, ModelError,
       StreamEvent, StreamOptions,
       ThinkingLevel, CachePolicy,
       TokenUsage, StopReason, ModelSpec,
       AnthropicAdapter, AnthropicConfig,
       OpenAiAdapter, OpenAiConfig,
       DeepSeekAdapter, DeepSeekConfig,
       OpenRouterAdapter, OpenRouterConfig,
       create_adapter, stream, call,
   };

   /// Backward-compatible alias.
   pub type ModelStreamChunk = StreamEvent;
   ```

3. **`src/model/anthropic.rs`** and **`src/model/openai.rs`**: delete. Code moves to providers.

4. **`src/tool/mod.rs`**: `ToolDef` and `JsonSchema` are now re-exported from providers via `model/mod.rs`. Update the `use` path. The `Tool` trait, `ToolMetadata`, `ToolOutput`, and other tool-specific types remain in core.

5. **`src/run.rs`**: update import paths. `ModelStreamChunk` usages either switch to `StreamEvent` or use the type alias. No logic changes.

6. **`src/events.rs`**: `ModelStreamChunk` in the `RuntimeEvent::ModelStreamChunk` variant — keep the variant name but use the type alias. Or rename the variant to `ModelStreamEvent` with a `delta: StreamEvent` field. Either works; variant rename is cleaner but a larger diff.

7. **`src/tool/mcp.rs`**: if it uses `ToolDef`, update the import path.

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
| `sse::parse_buffered_chunks` | **new** | Cross-boundary SSE parsing |
| `factory::routes_by_provider` | **new** | create_adapter routing |
| `factory::rejects_unknown_provider` | **new** | Error for unsupported provider |
| `factory::openrouter_preserves_full_model` | **new** | "openrouter/anthropic/claude-sonnet-4" |
| `anthropic::thinking_level_maps_to_budget` | **new** | ThinkingLevel → budget_tokens mapping |
| `anthropic::thinking_budget_override` | **new** | StreamOptions::thinking_budget_tokens overrides level default |
| `anthropic::include_thinking_false_maps_to_omitted` | **new** | include_thinking=false → display: "omitted" |
| `anthropic::cache_policy_auto_adds_cache_control` | **new** | CachePolicy::Auto adds top-level cache_control |
| `anthropic::cache_policy_long_sets_1h_ttl` | **new** | CachePolicy::Long → ttl: "1h" |
| `anthropic::cache_usage_mapped_to_token_usage` | **new** | cache_read/write_input_tokens → TokenUsage |
| `openai::thinking_level_maps_to_reasoning_effort` | **new** | ThinkingLevel → reasoning_effort |
| `openai::cache_tokens_reported` | **new** | cached_tokens → TokenUsage::cache_read_tokens |
| `deepseek::thinking_off_disables_reasoning` | **new** | ThinkingLevel::Off suppresses reasoning |
| `deepseek::strips_reasoning_from_input` | **new** | reasoning_content stripped from input messages |
| `deepseek::cache_hit_tokens_reported` | **new** | prompt_cache_hit_tokens → TokenUsage |
| `openrouter::reasoning_object_from_thinking_level` | **new** | ThinkingLevel → reasoning.effort |
| `complete::tx_none_skips_events` | **new** | complete(tx=None) returns response without streaming |
| `stream::tool_call_end_emitted` | **new** | ToolCallEnd emitted after tool call args are complete |
| `stream::parallel_tool_calls_end_each` | **new** | Each parallel tool call gets its own ToolCallEnd |
| `anthropic::temperature_forwarded` | **new** | StreamOptions::temperature → API `temperature` |
| `openai::temperature_forwarded` | **new** | StreamOptions::temperature → API `temperature` |
| `anthropic::adaptive_vs_enabled_mode` | **new** | Newer models use adaptive, older use enabled |

All SSE tests use the existing `serve_sse_once` pattern (bind to `127.0.0.1:0`, serve one HTTP response with SSE body).

### Core test changes

- Model-specific tests (SSE parsing, URL normalization) are removed from core — now covered by providers crate.
- Core retains `MockModelProvider`-based tests for run loop behavior. `MockModelProvider` stays in core (it implements `ModelAdapter` from providers).
- Integration tests (`e2e_validation.rs`, `v03_runtime.rs`) continue to work unchanged since they use `MockModelProvider`.

## Standalone usage example

```rust
use agent_runtime_providers::{
    create_adapter, stream, call,
    Message, ContentBlock, Role, StreamEvent,
    StreamOptions, ThinkingLevel, CachePolicy,
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
    let options = StreamOptions {
        thinking: ThinkingLevel::High,
        cache_policy: CachePolicy::Auto,
        ..Default::default()
    };

    let (fut, mut rx) = stream(adapter.as_ref(), &messages, &[], &options);
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

    // -- Non-streaming example (same interface, just use call()) --
    let response = call(adapter.as_ref(), &messages, &[], &options).await?;
    println!("Got {} content blocks", response.content.len());

    // -- Think deeply but hide thinking from output (lower latency) --
    let _fast = StreamOptions {
        thinking: ThinkingLevel::High,   // 怎么思考: deep
        include_thinking: false,          // 输出不包含思考
        ..Default::default()
    };

    // -- Advanced: explicit budget_tokens + long cache (Anthropic-only) --
    let _advanced = StreamOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: Some(16384),
        include_thinking: true,
        cache_policy: CachePolicy::Long,  // 1-hour cache
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
9. Update SDK import paths in `agent-runtime-py` and `agent-runtime-node`
10. Run `cargo test --workspace`, fix any remaining path issues
11. Run `cargo clippy --workspace -- -D warnings` and `cargo fmt --check`

## Future extensions

When adding heavyweight providers (AWS Bedrock, Google Vertex AI), introduce feature flags at that point:

```toml
[features]
default = []  # lightweight providers always compiled
bedrock = ["dep:aws-sdk-bedrockruntime", "dep:aws-config"]
vertex = ["dep:google-cloud-auth"]
```

This avoids pulling heavy SDK dependencies for users who only need reqwest-based providers.
