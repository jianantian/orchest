use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Language(pub String);

impl Language {
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Pcm16Le,
    WavPcm16Le,
    Mp3,
    OggOpus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioOutputConfig {
    pub format: AudioFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate_hz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u16>,
}

impl AudioOutputConfig {
    pub fn new(format: AudioFormat) -> Self {
        Self {
            format,
            sample_rate_hz: None,
            channels: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechControls {
    /// Portable speed range: `0.5..=2.0`.
    pub speed: f32,
    /// Portable pitch range in semitones: `-12.0..=12.0`.
    pub pitch: f32,
    /// Portable volume multiplier range: `0.0..=2.0`.
    pub volume: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default)]
    pub allow_semantic_coercions: bool,
}

impl Default for SpeechControls {
    fn default() -> Self {
        Self {
            speed: 1.0,
            pitch: 0.0,
            volume: 1.0,
            instruction: None,
            emotion: None,
            style: None,
            allow_semantic_coercions: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityPolicy {
    #[default]
    Coerce,
    Strict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionAdjustment {
    pub option: String,
    pub requested: Value,
    pub applied: Value,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TtsInput {
    Text(String),
    Ssml(String),
}

impl TtsInput {
    pub fn kind(&self) -> TtsInputKind {
        match self {
            Self::Text(_) => TtsInputKind::Text,
            Self::Ssml(_) => TtsInputKind::Ssml,
        }
    }

    pub fn char_count(&self) -> u64 {
        match self {
            Self::Text(text) | Self::Ssml(text) => text.chars().count() as u64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsInputKind {
    Text,
    Ssml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextChunk {
    pub text: String,
    pub is_final: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceKind {
    System,
    Cloned,
    Designed,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceGender {
    Male,
    Female,
    Neutral,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCatalogSource {
    ProviderMetadata,
    StaticCatalog,
    CallerConfig,
    ConservativeAssumption,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub provider: String,
    pub model: String,
    pub id: String,
    pub display_name: String,
    pub kind: VoiceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<VoiceGender>,
    #[serde(default)]
    pub languages: Vec<Language>,
    pub is_custom: bool,
    pub supports_instruction: bool,
    pub supports_emotion: bool,
    pub supports_style: bool,
    pub supports_cloning: bool,
    pub supports_design: bool,
    pub source: VoiceCatalogSource,
    #[serde(default)]
    pub provider_metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceSelection {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<VoiceKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
}

impl VoiceSelection {
    pub fn by_id(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: None,
            language: None,
        }
    }

    pub fn with_kind(mut self, kind: VoiceKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_language(mut self, language: Language) -> Self {
        self.language = Some(language);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input: TtsInput,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    #[serde(default)]
    pub compatibility: CompatibilityPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSynthesizeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input: TtsInput,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    #[serde(default)]
    pub compatibility: CompatibilityPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplexSynthesizeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    #[serde(default)]
    pub compatibility: CompatibilityPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVoicesRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<VoiceKind>,
    #[serde(default)]
    pub include_custom: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioData {
    Bytes(Bytes),
    Url {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsUsage {
    pub input_chars: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billable_chars: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate_micros: Option<u64>,
}

impl TtsUsage {
    pub fn for_text(text: &str, output_bytes: Option<u64>) -> Self {
        let input_chars = text.chars().count() as u64;
        Self {
            input_chars,
            billable_chars: Some(input_chars),
            audio_duration_ms: None,
            output_bytes,
            cost_estimate_micros: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsOperation {
    Batch,
    SingleStream,
    DuplexStream,
    ListVoices,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeResult {
    pub audio: AudioData,
    pub format: AudioFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub usage: TtsUsage,
    #[serde(default)]
    pub option_adjustments: Vec<OptionAdjustment>,
    #[serde(default)]
    pub provider_metadata: Value,
    pub telemetry: crate::observability::TtsTelemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsStreamSummary {
    pub format: AudioFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub usage: TtsUsage,
    #[serde(default)]
    pub option_adjustments: Vec<OptionAdjustment>,
    #[serde(default)]
    pub provider_metadata: Value,
    pub telemetry: crate::observability::TtsTelemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsModelCapabilities {
    pub batch_synthesis: bool,
    pub single_streaming: bool,
    pub duplex_streaming: bool,
    #[serde(default)]
    pub input_kinds: Vec<TtsInputKind>,
    #[serde(default)]
    pub languages: Vec<Language>,
    #[serde(default)]
    pub voice_kinds: Vec<VoiceKind>,
    #[serde(default)]
    pub batch_output_formats: Vec<AudioFormat>,
    #[serde(default)]
    pub stream_output_formats: Vec<AudioFormat>,
    pub supports_instruction: bool,
    pub supports_emotion: bool,
    pub supports_style: bool,
    pub supports_ssml: bool,
}

impl Default for TtsModelCapabilities {
    fn default() -> Self {
        Self {
            batch_synthesis: false,
            single_streaming: false,
            duplex_streaming: false,
            input_kinds: vec![TtsInputKind::Text],
            languages: Vec::new(),
            voice_kinds: vec![VoiceKind::System],
            batch_output_formats: Vec::new(),
            stream_output_formats: Vec::new(),
            supports_instruction: false,
            supports_emotion: false,
            supports_style: false,
            supports_ssml: false,
        }
    }
}
