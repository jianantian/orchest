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
    /// Stream a model response. Emits StreamEvent items to tx during streaming,
    /// then returns the complete ModelResponse.
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &StreamOptions,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<ModelResponse, ModelError>;

    /// Non-streaming convenience method. Default implementation creates a
    /// channel and discards the receiver.
    async fn call(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &StreamOptions,
    ) -> Result<ModelResponse, ModelError> {
        let (tx, _rx) = mpsc::channel(64);
        self.stream(messages, tools, options, tx).await
    }
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
    Thinking { text: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: Value },
}
```

`ContentBlock::Thinking` is new compared to the current core types. It persists thinking content in the message history rather than only surfacing it through streaming events. This enables:

- Context compaction that preserves reasoning traces
- Sub-agent message forwarding that includes thinking
- Session persistence of thinking blocks

### Thinking configuration

```rust
/// Primary way to control thinking/reasoning depth. Each adapter maps
/// these levels to provider-specific parameters. Not all providers
/// support all levels — adapters clamp to the nearest supported value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
}

/// Per-request options passed to `ModelAdapter::stream()` / `call()`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamOptions {
    /// Thinking depth. Default: `Medium`.
    pub thinking: ThinkingLevel,

    /// Advanced: override the provider's default token budget for this
    /// thinking level. Most models do not support this — only providers
    /// with explicit budget_tokens support (e.g., Anthropic) will honor it.
    /// Ignored when `thinking` is `Off`.
    pub thinking_budget_tokens: Option<u32>,

    /// Per-request max output tokens override. Falls back to the
    /// adapter's configured default when `None`.
    pub max_tokens: Option<u32>,
}
```

ThinkingLevel is the user-facing control. Each adapter maps it to provider-specific API parameters:

| Provider   | Mapping                                                                 |
|------------|-------------------------------------------------------------------------|
| Anthropic  | Maps to `thinking.type` + `thinking.budget_tokens`. Supports `budget_tokens` override. `Off` → no thinking block. |
| OpenAI     | Maps to `reasoning_effort`: Off→none, Minimal/Low→low, Medium→medium, High/XHigh→high. `budget_tokens` ignored. |
| DeepSeek   | Off→disable reasoning, all others→enable reasoning. No granular levels. `budget_tokens` ignored. |
| OpenRouter | Passes through to underlying model's native thinking parameters.        |

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
    ThinkingEnd,
    ToolCallArgsChunk { id: String, delta: String },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
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

ThinkingLevel mapping: Anthropic is the only provider that supports explicit `budget_tokens`. The adapter maps levels to token budgets (e.g., Off→omit thinking block, Minimal→1024, Low→4096, Medium→10240, High→32768, XHigh→max allowed). When `StreamOptions::thinking_budget_tokens` is set, it overrides the level's default budget.

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

Internally reuses SSE parsing from `sse.rs`. The adapter handles:
- URL normalization (append `/v1/chat/completions` if needed)
- `DEEPSEEK_API_KEY` env var fallback
- `reasoning_content` delta extraction

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

Reasoning support: when the underlying model supports reasoning, OpenRouter includes `reasoning` content in the response. Mapped to `StreamEvent::Thinking` and `ContentBlock::Thinking` using the same pattern as DeepSeek.

ThinkingLevel mapping: OpenRouter forwards thinking parameters to the underlying model. The adapter translates ThinkingLevel to the appropriate provider-specific format based on the model prefix (e.g., `anthropic/` models get Anthropic-style thinking config, `openai/` models get `reasoning_effort`). `budget_tokens` is forwarded when the underlying model supports it.

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
    tx: &mpsc::Sender<StreamEvent>,
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
       StreamEvent, StreamOptions, ThinkingLevel,
       TokenUsage, StopReason, ModelSpec,
       AnthropicAdapter, AnthropicConfig,
       OpenAiAdapter, OpenAiConfig,
       DeepSeekAdapter, DeepSeekConfig,
       OpenRouterAdapter, OpenRouterConfig,
       create_adapter,
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
| `openai::thinking_level_maps_to_reasoning_effort` | **new** | ThinkingLevel → reasoning_effort |
| `deepseek::thinking_off_disables_reasoning` | **new** | ThinkingLevel::Off suppresses reasoning |

All SSE tests use the existing `serve_sse_once` pattern (bind to `127.0.0.1:0`, serve one HTTP response with SSE body).

### Core test changes

- Model-specific tests (SSE parsing, URL normalization) are removed from core — now covered by providers crate.
- Core retains `MockModelProvider`-based tests for run loop behavior. `MockModelProvider` stays in core (it implements `ModelAdapter` from providers).
- Integration tests (`e2e_validation.rs`, `v03_runtime.rs`) continue to work unchanged since they use `MockModelProvider`.

## Standalone usage example

```rust
use agent_runtime_providers::{
    create_adapter, Message, ContentBlock, Role, StreamEvent,
    StreamOptions, ThinkingLevel,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create adapter from model string
    let adapter = create_adapter("deepseek/deepseek-chat", None)?;

    // Build messages
    let messages = vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text("You are helpful.".into())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello!".into())],
        },
    ];

    // Configure thinking level (primary API)
    let options = StreamOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };

    // Or with advanced budget_tokens override (only honored by Anthropic)
    let _advanced = StreamOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: Some(16384),
        ..Default::default()
    };

    // Stream response
    let (tx, mut rx) = mpsc::channel(64);
    let handle = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Text { delta } => print!("{delta}"),
                StreamEvent::ThinkingStart => print!("[thinking] "),
                StreamEvent::Thinking { delta } => print!("{delta}"),
                StreamEvent::ThinkingEnd => println!(" [/thinking]"),
                StreamEvent::Done { usage } => {
                    println!("\n({} in, {} out)", usage.input_tokens, usage.output_tokens);
                }
                _ => {}
            }
        }
    });

    let response = adapter.stream(&messages, &[], &options, tx).await?;
    handle.await?;

    println!("Stop reason: {:?}", response.stop_reason);
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
