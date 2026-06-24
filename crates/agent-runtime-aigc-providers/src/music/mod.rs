//! Music generation provider abstraction (aigc submodule).
//!
//! Parallel to [`crate::ImageProvider`] / [`crate::VideoProvider`].
//!
//! PRD delta 2: lives in `agent-runtime-aigc-providers` rather than a new
//! `agent-runtime-music-providers` crate. Step 3 of provider unification
//! will consolidate; adding a third generation-type trait next to image and
//! video is the cheaper path. Reuses [`AigcError`] + the existing
//! [`crate::AssetStore`] / [`crate::AssetRegistry`] for hex/url persistence.
//!
//! Four operations(Minimax 协议):
//! - `generate` — `POST /v1/music_generation` non-stream.
//! - `stream_generate` — same endpoint, `stream=true` + `output_format=hex`,
//!   returns a [`MusicStream`] of decoded chunks.
//! - `generate_lyrics` — `POST /v1/lyrics_generation`.
//! - `preprocess_cover` — `POST /v1/music_cover_preprocess`, returns the
//!   `cover_feature_id` used by the two-step cover flow.

pub mod minimax;

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::AigcError;

pub use minimax::{MinimaxMusicAdapter, MinimaxMusicConfig};

/// Output format requested from the upstream music generation endpoint
/// (`docs/external/minimax/music/generation.md:116-124`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MusicOutputFormat {
    /// Pre-signed URL valid for 24h; adapter persists via `AssetStore` when
    /// storage handles are configured.
    Url,
    /// Hex-encoded audio bytes returned inline. Streaming always uses this.
    #[default]
    Hex,
}

/// Source of reference audio for the `music-cover` family.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CoverAudioSource {
    /// `audio_url` field — URL pointing to the reference clip.
    Url { url: String },
    /// `audio_base64` field — raw base64 string.
    Base64 { data: String },
    /// `cover_feature_id` returned by a prior `preprocess_cover` call;
    /// enables the two-step cover flow with editable lyrics.
    FeatureId { id: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MusicAudioSetting {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateMusicRequest {
    /// `music-2.6` / `music-2.6-free` / `music-cover` / `music-cover-free`.
    pub model: String,
    /// Style/scene/mood description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Lyrics with structure tags. Required for vocal generations on
    /// `music-2.6`; optional for `music-cover` if reference audio carries lyrics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<String>,
    /// Hex (default) or URL output.
    #[serde(default)]
    pub output_format: MusicOutputFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_setting: Option<MusicAudioSetting>,
    #[serde(default)]
    pub aigc_watermark: bool,
    /// `music-2.6` only: auto-generate lyrics from prompt when `lyrics` is empty.
    #[serde(default, skip_serializing_if = "is_false")]
    pub lyrics_optimizer: bool,
    /// `music-2.6` only: instrumental output (no vocals).
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_instrumental: bool,
    /// `music-cover` family only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_audio: Option<CoverAudioSource>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Output of [`MusicProvider::generate`]. `Audio` is the decoded payload
/// (hex → Bytes, or the original/persisted URL); `extra_info` captures the
/// Minimax response metadata.
#[derive(Debug, Clone)]
pub struct GenerateMusicResult {
    pub audio: MusicAudio,
    pub extra_info: serde_json::Value,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MusicAudio {
    /// Hex-decoded raw bytes.
    Bytes(Bytes),
    /// Upstream URL (24h valid) when storage isn't configured, OR the
    /// adapter-controlled URL after persistence.
    Url(String),
}

#[derive(Debug)]
pub struct MusicStream {
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, AigcError>>,
}

impl MusicStream {
    pub fn new(rx: tokio::sync::mpsc::Receiver<Result<Bytes, AigcError>>) -> Self {
        Self { rx }
    }

    pub async fn recv(&mut self) -> Option<Result<Bytes, AigcError>> {
        self.rx.recv().await
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LyricsMode {
    #[default]
    WriteFullSong,
    Edit,
}

impl LyricsMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LyricsMode::WriteFullSong => "write_full_song",
            LyricsMode::Edit => "edit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateLyricsRequest {
    pub mode: LyricsMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Existing lyrics — only meaningful in `Edit` mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateLyricsResult {
    pub song_title: String,
    pub style_tags: String,
    pub lyrics: String,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverPreprocessRequest {
    /// Always `music-cover` per docs; kept configurable for `music-cover-free`.
    pub model: String,
    pub audio_source: CoverAudioSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverPreprocessResult {
    pub cover_feature_id: String,
    pub formatted_lyrics: String,
    /// JSON string of section types + timestamps (see docs).
    pub structure_result: String,
    pub audio_duration: f64,
    pub trace_id: Option<String>,
}

#[async_trait]
pub trait MusicProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    async fn generate(&self, req: GenerateMusicRequest) -> Result<GenerateMusicResult, AigcError>;
    async fn stream_generate(&self, req: GenerateMusicRequest) -> Result<MusicStream, AigcError>;
    async fn generate_lyrics(
        &self,
        req: GenerateLyricsRequest,
    ) -> Result<GenerateLyricsResult, AigcError>;
    async fn preprocess_cover(
        &self,
        req: CoverPreprocessRequest,
    ) -> Result<CoverPreprocessResult, AigcError>;
}
