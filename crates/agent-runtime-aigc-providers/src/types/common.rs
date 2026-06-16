//! Types shared between image and video generation: asset references,
//! provider-facing status/error types, and the runtime config used to build
//! either kind of provider from `create_image_provider_from_config` /
//! `create_video_provider_from_config`.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use bytes::Bytes;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AssetRef {
    Url(String),
    DataUrl(String),
    Base64 { data: String, mime_type: String },
    Bytes { bytes: Bytes, mime_type: String },
    LocalPath(String),
    Stored { asset_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AssetIngestSource {
    Url(String),
    DataUrl(String),
    Base64 { data: String, mime_type: String },
    Bytes { bytes: Bytes, mime_type: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    #[default]
    Coerce,
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptionAdjustment {
    pub option: String,
    pub requested: Value,
    pub applied: Value,
    pub reason: String,
}

/// Status shared by `ImageGenerationResponse` and `VideoGenerationResponse`
/// (the gateway's public-facing status, as opposed to `ProviderGenerationStatus`
/// which providers report).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GenerationStatus {
    Queued,
    Running,
    PersistingAssets,
    Completed,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CapabilitySource {
    #[default]
    Static,
    ProviderMetadata,
    Assumed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GenerationExecutionMode {
    Sync,
    Async,
    Stream,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct AigcError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_body: Option<Value>,
}

impl AigcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            provider: None,
            status: None,
            upstream_code: None,
            upstream_message: None,
            upstream_body: None,
        }
    }

    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }
}

/// Status a provider adapter reports for an async generation job — shared by
/// both image and video providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderGenerationStatus {
    Queued,
    Running,
    Completed,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AigcProviderRuntimeConfig {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default, with = "duration_millis_opt")]
    pub timeout: Option<Duration>,
    #[serde(default)]
    pub provider_options: Value,
}

pub(crate) mod duration_millis_opt {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(duration) => serializer.serialize_some(&(duration.as_millis() as u64)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<u64>::deserialize(deserializer)?;
        Ok(value.map(Duration::from_millis))
    }
}
