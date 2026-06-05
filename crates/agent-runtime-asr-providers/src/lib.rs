#![allow(clippy::result_large_err)]

pub mod config;
pub mod error;
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
pub use observability::AsrTelemetry;
pub use routing::{AsrGateway, AsrGatewayConfig, AsrRoute, AsrRouter};
pub use streaming::{AsrAudioSink, AsrEventStream, AsrStream};
pub use traits::AsrProvider;
pub use types::*;
