//! ASR provider gateway for the Orchest runtime: streaming speech-to-text.
//!
//! Standalone crate with zero workspace-internal dependencies. Entry points are
//! [`AsrGateway`] (routing + convenience surface) and the [`AsrProvider`] trait;
//! callers stream audio through [`AsrAudioSink`] and consume transcript events
//! through [`AsrEventStream`].
#![allow(clippy::result_large_err)]

pub mod catalog;
pub use catalog::{AsrModelCapabilitiesSummary, AsrModelEntry};

pub mod compatibility;
pub mod config;
pub mod error;
#[cfg(any(feature = "assemblyai", feature = "speechmatics"))]
mod http;
pub mod observability;
pub mod providers;
pub mod routing;
pub mod streaming;
pub mod traits;
pub mod types;

pub use config::{
    create_asr_provider_from_config, normalize_asr_provider_model, AsrProviderRuntimeConfig,
    NormalizedAsrProviderModel,
};
pub use error::{AsrError, AsrErrorCode};
pub use observability::{AsrTelemetry, AsrTelemetryBuilder};
pub use routing::{AsrGateway, AsrGatewayConfig, AsrRoute, AsrRouter};
pub use streaming::{AsrAudioSink, AsrEventStream, AsrStream};
pub use traits::AsrProvider;
pub use types::*;
