//! Model adapter re-exports from `agent-runtime-model`.

pub use agent_runtime_model::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, JsonSchema,
    Message, ModelAdapter, ModelCapabilities, ModelError, ModelPricing, ModelResponse, ModelSpec,
    OptionAdjustment, ProviderRuntimeConfig, ReasoningCapability, RequestOptions, Role, StopReason,
    StreamEvent, ThinkingLevel, TokenUsage, ToolDef, UpstreamErrorDetail,
};

/// Backward-compatible alias.
pub type ModelStreamChunk = StreamEvent;
