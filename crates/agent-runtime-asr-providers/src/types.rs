use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Primitive types
// ---------------------------------------------------------------------------

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkRegion(pub String);

impl NetworkRegion {
    pub fn new(region: impl Into<String>) -> Self {
        Self(region.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Pcm,
    Wav,
    Opus,
    Mp3,
    Ogg,
    Flac,
}

// ---------------------------------------------------------------------------
// Audio types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioInput {
    Bytes {
        #[serde(with = "serde_bytes_vec")]
        data: Vec<u8>,
        format: AudioFormat,
        sample_rate_hz: Option<u32>,
    },
    File {
        path: PathBuf,
        format: Option<AudioFormat>,
    },
    Url {
        url: String,
        format: Option<AudioFormat>,
    },
}

/// Lossy byte-vector serde used only for debug/log serialization.
///
/// `AudioInput::Bytes` can be large and is not intended to round-trip through
/// JSON persistence or transport in this crate, so serialization records only
/// the byte length and deserialization restores a zero-filled placeholder.
mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
        data.len().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let len = usize::deserialize(d)?;
        Ok(vec![0; len])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamingAudioFormat {
    Pcm16 { sample_rate_hz: u32, channels: u16 },
    Encoded { format: AudioFormat },
}

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub data: Bytes,
    pub timestamp_ms: Option<u64>,
    pub boundary: AudioChunkBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioChunkBoundary {
    None,
    Flush,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioTimelineMode {
    ContinuousRealtime,
    SparseSpeechOnly,
}

// ---------------------------------------------------------------------------
// Endpointing types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointingMode {
    ProviderDefault,
    AcousticSilence,
    Semantic,
    NaturalSegmenting,
    ProviderDisabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointingOptions {
    pub mode: EndpointingMode,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_opt_duration_ms"
    )]
    pub silence_timeout: Option<Duration>,
}

// ---------------------------------------------------------------------------
// Options / Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalResultScope {
    Segment,
    Stream,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default)]
    pub hot_words: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_prompt: Option<String>,
    #[serde(default)]
    pub code_switching: bool,
    #[serde(default = "default_true")]
    pub punctuate: bool,
    #[serde(default = "default_true")]
    pub interim_results: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpointing: Option<EndpointingOptions>,
    #[serde(default)]
    pub word_timestamps: bool,
    #[serde(default)]
    pub speaker_diarization: bool,
    #[serde(default = "default_final_result_scope")]
    pub final_result_scope: FinalResultScope,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_opt_duration_ms"
    )]
    pub flush_timeout: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_final_result_scope() -> FinalResultScope {
    FinalResultScope::Segment
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: None,
            hot_words: Vec::new(),
            context_prompt: None,
            code_switching: false,
            punctuate: true,
            interim_results: true,
            endpointing: None,
            word_timestamps: false,
            speaker_diarization: false,
            final_result_scope: FinalResultScope::Segment,
            flush_timeout: None,
            trace_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityPolicy {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub audio: AudioInput,
    pub options: TranscribeOptions,
    #[serde(default = "default_compatibility")]
    pub compatibility: CompatibilityPolicy,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTranscribeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub format: StreamingAudioFormat,
    pub timeline: AudioTimelineMode,
    pub options: TranscribeOptions,
    #[serde(default = "default_compatibility")]
    pub compatibility: CompatibilityPolicy,
    #[serde(default)]
    pub provider_options: Value,
}

fn default_compatibility() -> CompatibilityPolicy {
    CompatibilityPolicy::Coerce
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTimestamp {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerSegment {
    pub speaker_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AsrUsage {
    pub audio_duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billable_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_chars: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate_micros: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub words: Vec<WordTimestamp>,
    #[serde(default)]
    pub speakers: Vec<SpeakerSegment>,
    pub audio_duration_ms: u64,
    pub processing_latency_ms: u64,
    pub usage: AsrUsage,
    #[serde(default)]
    pub option_adjustments: Vec<OptionAdjustment>,
    pub telemetry: crate::observability::AsrTelemetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsrFinalReason {
    CallerFlush,
    CallerEnd,
    ProviderEndpoint,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrFinalOutput {
    pub trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<String>,
    pub reason: AsrFinalReason,
    pub result: TranscribeResult,
}

// ---------------------------------------------------------------------------
// Stream event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptStability {
    Provisional,
    Committed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptUpdateKind {
    Snapshot,
    Append,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AsrStreamEvent {
    RouteSelected {
        trace_id: String,
        model: String,
    },
    Started {
        trace_id: String,
        model: String,
    },
    TranscriptUpdate {
        trace_id: String,
        segment_id: Option<String>,
        text: String,
        stability: TranscriptStability,
        update_kind: TranscriptUpdateKind,
    },
    EndOfSpeech {
        trace_id: String,
        segment_id: Option<String>,
    },
    AsrFinal {
        final_output: Box<AsrFinalOutput>,
    },
    Error {
        trace_id: String,
        error: crate::error::AsrError,
        fatal: bool,
    },
}

// ---------------------------------------------------------------------------
// Capability types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleRateSupport {
    Any,
    Exact(Vec<u32>),
    Range { min: u32, max: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelSupport {
    Any,
    Exact(Vec<u16>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInputCapability {
    pub format: AudioFormat,
    pub sample_rates_hz: SampleRateSupport,
    pub channels: ChannelSupport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    Static,
    ProviderMetadata,
    Assumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionReuse {
    NotReusable,
    ReusableAfterTerminalFinal,
    ReusableAfterProviderTaskFinished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrModelCapabilities {
    pub languages: Vec<Language>,
    pub streaming: bool,
    pub batch: bool,
    #[serde(default)]
    pub streaming_inputs: Vec<AudioInputCapability>,
    #[serde(default)]
    pub batch_inputs: Vec<AudioInputCapability>,
    #[serde(default)]
    pub audio_timeline_modes: Vec<AudioTimelineMode>,
    pub interim_results: bool,
    #[serde(default)]
    pub endpointing_modes: Vec<EndpointingMode>,
    pub segment_flush: bool,
    pub multi_segment_streaming: bool,
    pub connection_reuse: ConnectionReuse,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub confidence: bool,
    pub code_switching: bool,
    pub hot_words: bool,
    pub context_prompt: bool,
    #[serde(default)]
    pub provider_option_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_flush_timeout_ms: Option<u64>,
    pub source: CapabilitySource,
    #[serde(default)]
    pub diagnostic_metadata: Value,
}

// ---------------------------------------------------------------------------
// Duration serde helper (milliseconds)
// ---------------------------------------------------------------------------

mod serde_opt_duration_ms {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(val: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match val {
            Some(d) => d.as_millis().serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let ms: Option<u64> = Option::deserialize(d)?;
        Ok(ms.map(Duration::from_millis))
    }
}
