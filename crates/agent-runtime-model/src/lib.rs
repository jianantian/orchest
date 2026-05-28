pub mod adapter;
pub mod error;
pub mod options;
pub mod response;
pub mod stream;
pub mod types;

pub use adapter::ModelAdapter;
pub use error::{ModelError, UpstreamErrorDetail};
pub use options::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ModelCapabilities,
    ModelPricing, ReasoningCapability, RequestOptions, ThinkingLevel,
};
pub use response::{ModelResponse, OptionAdjustment, StopReason, TokenUsage};
pub use stream::StreamEvent;
pub use types::{
    ContentBlock, JsonSchema, Message, ModelSpec, ProviderRuntimeConfig, Role, ToolDef,
};
