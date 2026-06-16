//! Video generation (async task creation + polling) request/response types.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::common::{duration_millis_opt, AssetRef, GenerationStatus, ProviderGenerationStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoGenerationRequest {
    pub content: Vec<VideoContentItem>,
    #[serde(default)]
    pub generation_config: VideoGenerationConfig,
    #[serde(default)]
    pub execution_config: VideoExecutionConfig,
    #[serde(default)]
    pub provider_options: Value,
}

/// Polling parameters for the gateway's wait loop. Defaults are tuned for
/// video generation (which takes tens of seconds to a few minutes), unlike
/// `GenerationExecutionConfig`'s image-oriented defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoExecutionConfig {
    #[serde(default, with = "duration_millis_opt")]
    pub poll_interval: Option<Duration>,
    #[serde(default, with = "duration_millis_opt")]
    pub timeout: Option<Duration>,
}

impl Default for VideoExecutionConfig {
    fn default() -> Self {
        Self {
            poll_interval: Some(Duration::from_secs(5)),
            timeout: Some(Duration::from_secs(600)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VideoContentItem {
    Text {
        text: String,
    },
    Image {
        asset: AssetRef,
        role: VideoImageRole,
    },
    Video {
        asset: AssetRef,
    },
    Audio {
        asset: AssetRef,
    },
    DraftTask {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VideoImageRole {
    FirstFrame,
    LastFrame,
    ReferenceImage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct VideoGenerationConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
    /// Seconds; `-1` requests provider-chosen ("auto") duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default)]
    pub camera_fixed: bool,
    #[serde(default)]
    pub watermark: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_audio: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub return_last_frame: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderVideoJob {
    pub id: String,
    pub status: ProviderGenerationStatus,
    /// Raw provider status string (e.g. "cancelled", "expired") preserved
    /// alongside the normalized `status` above, since `ProviderGenerationStatus`
    /// doesn't have a dedicated variant for every provider-specific state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Default)]
pub struct VideoTaskListQuery {
    pub page_num: Option<u32>,
    pub page_size: Option<u32>,
    pub status: Option<String>,
    pub task_ids: Vec<String>,
    pub model: Option<String>,
}

/// Public response from `VideoGateway::generate` — the provider's own
/// (24h-expiry) URLs have already been downloaded and persisted to our own
/// asset store; only controlled, signed/public URLs are exposed here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoGenerationResponse {
    pub job_id: String,
    pub status: GenerationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<GeneratedVideoAsset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame: Option<GeneratedVideoAsset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneratedVideoAsset {
    pub asset_id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}
