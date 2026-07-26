//! `orchest-protocol` — the provider-unification spine (v0.9.12).
//!
//! The leaf crate every layer and consumer speaks: the content model
//! ([`ContentBlock`]), the unified capability traits ([`ChatModel`], [`Asr`],
//! [`Tts`], [`VoiceManager`], [`RealtimeSession`], [`GenTask`]), the unified
//! [`StreamEvent`] model, the unified [`CapabilityDescriptor`], and the unified
//! [`ProtocolError`]. Evolved from `agent-runtime-model`, which now re-exports
//! this crate as a deprecated shell (removed in Issue 008). No runtime deps.

pub mod adapter;
pub mod capability;
pub mod descriptor;
pub mod error;
pub mod options;
pub mod response;
pub mod stream;
pub mod types;

// --- chat ---
pub use adapter::ChatModel;

/// Deprecated alias for [`ChatModel`] — renamed in v0.9.12 (provider
/// unification). Existing `impl ModelAdapter for X` keeps working through this
/// re-export; new code should name `ChatModel`. Removed in Issue 008.
pub use adapter::ChatModel as ModelAdapter;

// --- other capability traits + their IO shapes ---
pub use capability::{
    Asr, GenAsset, GenAssetRole, GenHandle, GenRequest, GenResult, GenStatus, GenTask, Language,
    MusicParams, RealtimeHandle, RealtimeSession, SessionInput, StreamingTranscribeRequest,
    SynthesizeRequest, SynthesizeResult, TimedSegment, TimedText, TranscribeRequest,
    TranscribeResult, Tts, VocalGender, VoiceManager,
};

// --- descriptor ---
pub use descriptor::{
    Capability, CapabilityDescriptor, CapabilityExt, CapabilitySource, CatalogEntry,
    ChatCapabilityExt, Modality,
};

// --- error ---
pub use error::{ErrorCode, ModelError, ProtocolError, UpstreamErrorDetail};

// --- options / pricing ---
pub use options::{
    CacheCapability, CachePolicy, CompatibilityPolicy, ModelCapabilities, ModelPricing,
    PricingRates, PricingTier, ReasoningCapability, RequestOptions, ThinkingLevel,
};

// --- response ---
pub use response::{ModelResponse, OptionAdjustment, StopReason, TokenUsage};

// --- streaming event model ---
pub use stream::{
    AudioFormat, CapabilityEventExt, EventStream, LifecycleEvent, SegmentRef, StreamEvent,
    TranscriptStability, TranscriptUpdateKind,
};

// --- content model ---
pub use types::{
    ContentBlock, JsonSchema, MediaSource, Message, ModelSpec, ProviderRuntimeConfig, Role, ToolDef,
};
