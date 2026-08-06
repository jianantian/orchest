//! Model adapter re-exports from `orchest-protocol` (the provider-unification
//! spine). `core` depends only on the spine — never on a concrete provider crate.

pub use orchest_protocol::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, JsonSchema,
    MediaSource, Message, ModelAdapter, ModelCapabilities, ModelError, ModelPricing, ModelResponse,
    ModelSpec, OptionAdjustment, ProviderRuntimeConfig, ReasoningCapability, RequestOptions,
    ResponseFormat, Role, StopReason, StreamEvent, ThinkingLevel, TokenUsage, ToolDef,
    UpstreamErrorDetail,
};

/// Backward-compatible alias.
pub type ModelStreamChunk = StreamEvent;
