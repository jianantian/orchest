use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// ModelAdapter trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ModelAdapter: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> ModelCapabilities;

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError>;
}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentBlock {
    Text(String),
    Thinking {
        text: Option<String>,
        signature: Option<String>,
        provider_details: Option<Value>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
    },
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
    Max,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CachePolicy {
    None,
    #[default]
    Auto,
    Long,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    #[default]
    Coerce,
    Strict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CapabilitySource {
    #[default]
    Static,
    ProviderMetadata,
    Assumed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestOptions {
    pub thinking: ThinkingLevel,
    pub thinking_budget_tokens: Option<u32>,
    pub include_thinking: bool,
    pub compatibility_policy: CompatibilityPolicy,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
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

// ---------------------------------------------------------------------------
// Capability metadata
// ---------------------------------------------------------------------------

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million_usd: Option<f64>,
}

impl ModelPricing {
    pub fn calculate(&self, usage: &TokenUsage) -> f64 {
        let base = usage.input_tokens as f64 * self.input_per_million_usd / 1_000_000.0
            + usage.output_tokens as f64 * self.output_per_million_usd / 1_000_000.0;
        let cache_read = usage.cache_read_tokens as f64
            * self.cache_read_per_million_usd.unwrap_or(0.0)
            / 1_000_000.0;
        let cache_write = usage.cache_write_tokens as f64
            * self.cache_write_per_million_usd.unwrap_or(0.0)
            / 1_000_000.0;
        base + cache_read + cache_write
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptionAdjustment {
    pub option: String,
    pub requested: Value,
    pub applied: Value,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Streaming events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamEvent {
    Text {
        delta: String,
    },
    ThinkingStart,
    Thinking {
        delta: String,
    },
    ThinkingEnd {
        signature: Option<String>,
        provider_details: Option<Value>,
    },
    ToolUseStart {
        id: String,
        name: String,
    },
    ToolUseArgsChunk {
        id: String,
        delta: String,
    },
    ToolUseEnd {
        id: String,
    },
    Done {
        usage: TokenUsage,
    },
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub option_adjustments: Vec<OptionAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub details: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

pub type JsonSchema = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

// ---------------------------------------------------------------------------
// Model spec
// ---------------------------------------------------------------------------

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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProviderRuntimeConfig {
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl From<ModelSpec> for ProviderRuntimeConfig {
    fn from(value: ModelSpec) -> Self {
        Self {
            model: format!("{}/{}", value.provider, value.model),
            api_key: None,
            api_key_env: value.api_key_env,
            api_url: value.api_url,
            max_tokens: value.max_tokens,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_default_is_medium() {
        assert_eq!(ThinkingLevel::default(), ThinkingLevel::Medium);
    }

    #[test]
    fn cache_policy_default_is_auto() {
        assert_eq!(CachePolicy::default(), CachePolicy::Auto);
    }

    #[test]
    fn compatibility_policy_default_is_coerce() {
        assert_eq!(CompatibilityPolicy::default(), CompatibilityPolicy::Coerce);
    }

    #[test]
    fn request_options_default() {
        let opts = RequestOptions::default();
        assert_eq!(opts.thinking, ThinkingLevel::Medium);
        assert!(opts.include_thinking);
        assert_eq!(opts.compatibility_policy, CompatibilityPolicy::Coerce);
        assert_eq!(opts.cache_policy, CachePolicy::Auto);
        assert!(opts.thinking_budget_tokens.is_none());
        assert!(opts.max_tokens.is_none());
        assert!(opts.temperature.is_none());
        assert!(opts.top_p.is_none());
    }

    #[test]
    fn model_error_internal_constructor() {
        let err = ModelError::internal("something failed", "test_error");
        assert_eq!(err.message, "something failed");
        assert_eq!(err.code.as_deref(), Some("test_error"));
        assert!(err.provider.is_none());
        assert!(err.status.is_none());
        assert!(err.upstream_code.is_none());
        assert!(err.upstream_message.is_none());
        assert!(err.upstream_body.is_none());
    }

    #[test]
    fn model_error_display() {
        let err = ModelError::internal("something failed", "test_error");
        assert_eq!(format!("{err}"), "something failed");
    }

    #[test]
    fn token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.reasoning_tokens, 0);
        assert_eq!(usage.cache_read_tokens, 0);
        assert_eq!(usage.cache_write_tokens, 0);
        assert!(usage.details.is_empty());
    }

    #[test]
    fn model_pricing_calculate_sonnet() {
        let pricing = ModelPricing {
            input_per_million_usd: 3.0,
            output_per_million_usd: 15.0,
            cache_read_per_million_usd: None,
            cache_write_per_million_usd: None,
        };
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        let cost = pricing.calculate(&usage);
        assert!((cost - 18.0).abs() < 1e-10);
    }

    #[test]
    fn model_pricing_calculate_with_cache() {
        let pricing = ModelPricing {
            input_per_million_usd: 3.0,
            output_per_million_usd: 15.0,
            cache_read_per_million_usd: Some(0.3),
            cache_write_per_million_usd: Some(3.75),
        };
        let usage = TokenUsage {
            input_tokens: 500_000,
            output_tokens: 100_000,
            cache_read_tokens: 200_000,
            cache_write_tokens: 50_000,
            ..Default::default()
        };
        let cost = pricing.calculate(&usage);
        let expected = 500_000.0 * 3.0 / 1_000_000.0
            + 100_000.0 * 15.0 / 1_000_000.0
            + 200_000.0 * 0.3 / 1_000_000.0
            + 50_000.0 * 3.75 / 1_000_000.0;
        assert!((cost - expected).abs() < 1e-10);
    }

    #[test]
    fn content_block_thinking_serde_roundtrip() {
        let block = ContentBlock::Thinking {
            text: Some("let me think...".into()),
            signature: Some("sig123".into()),
            provider_details: Some(serde_json::json!({"key": "value"})),
        };
        let json = serde_json::to_string(&block).unwrap();
        let restored: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, restored);
    }

    #[test]
    fn message_serde_roundtrip() {
        let msg = Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hello".into())],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn stream_event_done_serde_roundtrip() {
        let event = StreamEvent::Done {
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: StreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn stream_event_thinking_end_with_signature_serde_roundtrip() {
        let event = StreamEvent::ThinkingEnd {
            signature: Some("opaque-sig-abc".into()),
            provider_details: Some(serde_json::json!({"reasoning_id": "r_123"})),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: StreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn model_response_empty_adjustments_omitted_from_json() {
        let resp = ModelResponse {
            content: vec![ContentBlock::Text("hi".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        };
        let json_val: Value = serde_json::to_value(&resp).unwrap();
        assert!(json_val.get("option_adjustments").is_none());
    }

    #[test]
    fn model_response_with_adjustments_includes_field() {
        let resp = ModelResponse {
            content: vec![ContentBlock::Text("hi".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![OptionAdjustment {
                option: "thinking".into(),
                requested: serde_json::json!("High"),
                applied: serde_json::json!("Medium"),
                reason: "unsupported_effort".into(),
            }],
        };
        let json_val: Value = serde_json::to_value(&resp).unwrap();
        assert!(json_val.get("option_adjustments").is_some());
    }

    #[test]
    fn stop_reason_other_serde_preserves_string() {
        let reason = StopReason::Other("custom_reason".into());
        let json = serde_json::to_string(&reason).unwrap();
        let restored: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(reason, restored);
        if let StopReason::Other(s) = restored {
            assert_eq!(s, "custom_reason");
        } else {
            panic!("expected Other variant");
        }
    }

    #[test]
    fn model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(!caps.streaming);
        assert!(!caps.tool_use);
        assert!(!caps.parallel_tool_use);
        assert!(caps.max_output_tokens.is_none());
        assert!(caps.context_window_size.is_none());
    }

    #[test]
    fn tool_def_serde_roundtrip() {
        let tool = ToolDef {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let restored: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(tool, restored);
    }

    #[test]
    fn model_spec_optional_fields_deserialize() {
        let json = r#"{"provider":"anthropic","model":"claude-sonnet-4"}"#;
        let spec: ModelSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.provider, "anthropic");
        assert_eq!(spec.model, "claude-sonnet-4");
        assert!(spec.api_key_env.is_none());
        assert!(spec.api_url.is_none());
        assert!(spec.max_tokens.is_none());
        assert!(spec.context_window_size.is_none());
    }
}
