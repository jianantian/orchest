use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageOperation {
    TextToImage,
    ImageToImage,
    EditImage,
    Upscale,
    FaceSwap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageInputRole {
    Source,
    Reference,
    Mask,
    Font,
    SuperResolutionReference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageInput {
    pub role: ImageInputRole,
    pub asset: AssetRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageGenerationRequest {
    pub operation: ImageOperation,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default)]
    pub inputs: Vec<ImageInput>,
    #[serde(default)]
    pub generation_config: ImageGenerationConfig,
    #[serde(default)]
    pub execution_config: GenerationExecutionConfig,
    #[serde(default)]
    pub output_config: ImageOutputConfig,
    #[serde(default)]
    pub compatibility_policy: CompatibilityPolicy,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageGenerationConfig {
    #[serde(default)]
    pub size: ImageSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageQuality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ImageFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageBackground>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<SafetyConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<ImageEditConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ImageStyleConfig>,
}

impl Default for ImageGenerationConfig {
    fn default() -> Self {
        Self {
            size: ImageSize::Auto,
            count: Some(1),
            quality: None,
            format: None,
            background: None,
            seed: None,
            safety: None,
            edit: None,
            style: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ImageSize {
    #[default]
    Auto,
    Pixels {
        width: u32,
        height: u32,
    },
    AspectRatio(String),
    ResolutionTier(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageQuality {
    Low,
    Medium,
    High,
    Hd,
    Standard,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageBackground {
    Auto,
    Transparent,
    Opaque,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SafetyConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImageEditConfig {
    #[serde(default)]
    pub regions: Vec<ImageRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ImageRegion {
    BoundingBox {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Mask,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImageStyleConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default)]
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationExecutionConfig {
    #[serde(default)]
    pub prefer_async: bool,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_image_count: Option<u32>,
    #[serde(default, with = "duration_millis_opt")]
    pub poll_interval: Option<Duration>,
    #[serde(default, with = "duration_millis_opt")]
    pub timeout: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

impl Default for GenerationExecutionConfig {
    fn default() -> Self {
        Self {
            prefer_async: false,
            stream: false,
            partial_image_count: None,
            poll_interval: Some(Duration::from_millis(500)),
            timeout: Some(Duration::from_secs(120)),
            webhook_url: None,
            user: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageOutputConfig {
    #[serde(default)]
    pub delivery: ImageOutputDelivery,
}

impl Default for ImageOutputConfig {
    fn default() -> Self {
        Self {
            delivery: ImageOutputDelivery::Url,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ImageOutputDelivery {
    #[default]
    Url,
    Base64,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageGenerationResponse {
    pub job_id: String,
    pub status: GenerationStatus,
    pub images: Vec<GeneratedImage>,
    #[serde(default)]
    pub option_adjustments: Vec<OptionAdjustment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ImageUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneratedImage {
    pub asset_id: String,
    pub output: ImageOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ImageOutput {
    Url(ImageUrlOutput),
    Base64(ImageBase64Output),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageUrlOutput {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageBase64Output {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GenerationStatus {
    Queued,
    Running,
    PersistingAssets,
    Completed,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ImageGenerationEvent {
    Status { status: GenerationStatus },
    PartialImage { image: GeneratedImage },
    Completed { response: ImageGenerationResponse },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageModelCapabilities {
    pub provider: String,
    pub model: String,
    pub operations: HashMap<String, ImageOperationCapability>,
    #[serde(default)]
    pub source: CapabilitySource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CapabilitySource {
    #[default]
    Static,
    ProviderMetadata,
    Assumed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageOperationCapability {
    pub operation: ImageOperation,
    #[serde(default)]
    pub execution_modes: Vec<GenerationExecutionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_outputs: Option<u32>,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_transparent_background: bool,
    #[serde(default)]
    pub supported_formats: Vec<ImageFormat>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GenerationExecutionMode {
    Sync,
    Async,
    Stream,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImageUsage {
    pub image_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub details: HashMap<String, Value>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderImageJob {
    pub id: String,
    pub status: ProviderGenerationStatus,
    #[serde(default)]
    pub assets: Vec<ProviderAsset>,
    #[serde(default)]
    pub events: Vec<ProviderImageEvent>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ImageUsage>,
    #[serde(default)]
    pub option_adjustments: Vec<OptionAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderAsset {
    pub source: AssetIngestSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderImageEvent {
    Status { status: ProviderGenerationStatus },
    PartialAsset { asset: ProviderAsset },
}

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

mod duration_millis_opt {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(delivery: ImageOutputDelivery) -> ImageGenerationRequest {
        ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "a red cube".into(),
            negative_prompt: Some("blur".into()),
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: GenerationExecutionConfig::default(),
            output_config: ImageOutputConfig { delivery },
            compatibility_policy: CompatibilityPolicy::Coerce,
            provider_options: serde_json::json!({}),
        }
    }

    #[test]
    fn request_serde_roundtrip() {
        let request = sample_request(ImageOutputDelivery::Url);
        let json = serde_json::to_string(&request).unwrap();
        let restored: ImageGenerationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, restored);
    }

    #[test]
    fn response_serde_roundtrip() {
        let response = ImageGenerationResponse {
            job_id: "job".into(),
            status: GenerationStatus::Completed,
            images: vec![GeneratedImage {
                asset_id: "asset".into(),
                output: ImageOutput::Url(ImageUrlOutput {
                    url: "https://assets.example/a.png".into(),
                    expires_at: None,
                }),
                mime_type: Some("image/png".into()),
                width: Some(1024),
                height: Some(1024),
            }],
            option_adjustments: vec![],
            usage: Some(ImageUsage {
                image_count: 1,
                ..Default::default()
            }),
        };
        let json = serde_json::to_string(&response).unwrap();
        let restored: ImageGenerationResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, restored);
    }

    #[test]
    fn provider_job_serde_roundtrip() {
        let job = ProviderImageJob {
            id: "job".into(),
            status: ProviderGenerationStatus::Completed,
            assets: vec![ProviderAsset {
                source: AssetIngestSource::Url("https://provider/image.png".into()),
                mime_type: Some("image/png".into()),
                width: None,
                height: None,
                expires_at: None,
                metadata: serde_json::json!({}),
            }],
            events: vec![],
            metadata: serde_json::json!({}),
            usage: None,
            option_adjustments: vec![],
        };
        let json = serde_json::to_string(&job).unwrap();
        let restored: ProviderImageJob = serde_json::from_str(&json).unwrap();
        assert_eq!(job, restored);
    }

    #[test]
    fn generated_image_asset_id_exists_for_url_and_base64() {
        let url = GeneratedImage {
            asset_id: "asset-url".into(),
            output: ImageOutput::Url(ImageUrlOutput {
                url: "https://assets.example/a.png".into(),
                expires_at: None,
            }),
            mime_type: None,
            width: None,
            height: None,
        };
        let base64 = GeneratedImage {
            asset_id: "asset-b64".into(),
            output: ImageOutput::Base64(ImageBase64Output {
                data: "aGVsbG8=".into(),
                mime_type: "image/png".into(),
            }),
            mime_type: None,
            width: None,
            height: None,
        };
        assert!(!url.asset_id.is_empty());
        assert!(!base64.asset_id.is_empty());
    }

    #[test]
    fn public_url_output_does_not_expose_storage_descriptors() {
        let image = GeneratedImage {
            asset_id: "asset".into(),
            output: ImageOutput::Url(ImageUrlOutput {
                url: "https://assets.example/a.png".into(),
                expires_at: None,
            }),
            mime_type: None,
            width: None,
            height: None,
        };
        let json = serde_json::to_value(&image).unwrap();
        let serialized = json.to_string();
        for forbidden in [
            "bucket",
            "object_key",
            "endpoint",
            "access_key",
            "provider_url",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn capabilities_serde_roundtrip() {
        let mut operations = HashMap::new();
        operations.insert(
            "text_to_image".into(),
            ImageOperationCapability {
                operation: ImageOperation::TextToImage,
                execution_modes: vec![GenerationExecutionMode::Sync],
                max_outputs: Some(4),
                supports_streaming: false,
                supports_transparent_background: true,
                supported_formats: vec![ImageFormat::Png],
                metadata: Value::Null,
            },
        );
        let caps = ImageModelCapabilities {
            provider: "test".into(),
            model: "model".into(),
            operations,
            source: CapabilitySource::Static,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let restored: ImageModelCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, restored);
    }
}
