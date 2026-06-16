//! Image generation request/response/capability types.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::common::{
    duration_millis_opt, AssetIngestSource, AssetRef, CapabilitySource, CompatibilityPolicy,
    GenerationExecutionMode, GenerationStatus, OptionAdjustment, ProviderGenerationStatus,
};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ImageGenerationEvent {
    Status { status: GenerationStatus },
    PartialImage { image: GeneratedImage },
    Completed { response: ImageGenerationResponse },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageModelCapabilities {
    pub operations: HashMap<String, ImageOperationCapability>,
    #[serde(default)]
    pub source: CapabilitySource,
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

#[cfg(test)]
mod tests;
