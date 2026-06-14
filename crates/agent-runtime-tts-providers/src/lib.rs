//! TTS provider gateway for standalone text-to-speech synthesis.
//!
//! This crate is intentionally independent from `agent-runtime-core`. It owns
//! provider-neutral TTS requests, routing, voice catalog handling, stream
//! contracts, telemetry summaries, and feature-gated provider adapters.
#![allow(clippy::result_large_err)]

pub mod catalog;
pub use catalog::{TtsModelCapabilitiesSummary, TtsModelEntry};

pub mod config;
pub mod error;
pub mod observability;
pub mod providers;
pub mod routing;
pub mod streaming;
pub mod traits;
pub mod types;
pub mod voices;

pub use config::{
    create_tts_provider_from_config, normalize_tts_provider_model, validate_direct_model_selector,
    NormalizedTtsProviderModel, TtsProviderRuntimeConfig,
};
pub use error::{redact_secrets, TtsError, TtsErrorCode};
pub use observability::{TtsTelemetry, TtsTelemetryBuilder};
pub use routing::{TtsGateway, TtsGatewayConfig, TtsRoute, TtsRouter};
pub use streaming::{
    TtsDuplexStream, TtsEventStream, TtsOutputStream, TtsStreamEvent, TtsTextSink,
};
pub use traits::TtsProvider;
pub use types::*;
pub use voices::{filter_voices, resolve_voice_for_request};
