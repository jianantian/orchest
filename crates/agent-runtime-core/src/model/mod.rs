pub use agent_runtime_providers::{
    chat, create_adapter, stream_chat, AnthropicAdapter, AnthropicConfig, CacheCapability,
    CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, DeepSeekAdapter,
    DeepSeekConfig, JsonSchema, Message, ModelAdapter, ModelCapabilities, ModelError,
    ModelResponse, ModelSpec, OpenAiAdapter, OpenAiConfig, OpenRouterAdapter, OpenRouterConfig,
    OptionAdjustment, ReasoningCapability, RequestOptions, Role, StopReason, StreamEvent,
    ThinkingLevel, TokenUsage, ToolDef,
};

/// Backward-compatible alias.
pub type ModelStreamChunk = StreamEvent;
