//! Capability traits beyond chat: the **new** `RealtimeSession` and `GenTask`
//! (lifted from the concrete `VolcengineRealtimeSession` / `ImageGateway`), plus
//! the `Asr` / `Tts` / `VoiceManager` shapes that the satellite providers
//! converge onto in Issue 006.
//!
//! Designed in `docs/archive/iteration/v0_9_12/issues/001-protocol-design/design.md` §4.
//! Parallel, opt-in traits — no god-trait. `ChatModel` itself lives in
//! `crate::adapter`.

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::descriptor::CapabilityDescriptor;
use crate::error::ProtocolError;
use crate::stream::{AudioFormat, EventStream};

// ===========================================================================
// Realtime / omni — the omni acceptance ruler lands here
// ===========================================================================

/// What a caller sends **into** a live duplex session. The send side is one
/// channel so a mid-stream `ToolResult` never blocks outgoing audio (omni ruler,
/// design §6.1). Lifted from `VolcengineRealtimeSession`'s
/// `send_audio_chunk`/`interrupt`/text-input surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionInput {
    /// A chunk of input audio (e.g. microphone PCM). ← `send_audio_chunk`.
    Audio(Bytes),
    /// A text turn / instruction injected into the session.
    Text(String),
    /// A tool result for a mid-stream `ToolUse*`, fed back without stopping audio.
    ToolResult { tool_use_id: String, content: Value },
    /// Client-initiated barge-in. ← `interrupt`.
    Interrupt,
}

/// A live full-duplex session: send [`SessionInput`], pull the unified
/// [`EventStream`]. The omni openspeech impl (Issue 006) and any future realtime
/// dialect implement this; the session is handed back already started (factory
/// performs the handshake), mirroring `start_stream`/`start_duplex_stream`.
#[async_trait]
pub trait RealtimeSession: Send + Sync {
    /// Feed input into the session. Returns once accepted (does not await the
    /// model's response — that arrives on [`RealtimeSession::events`]).
    async fn send(&self, input: SessionInput) -> Result<(), ProtocolError>;

    /// The pulled stream of unified events (audio out, transcript, model text,
    /// mid-stream tool use, lifecycle). Read concurrently with `send`.
    fn events(&mut self) -> &mut EventStream;

    /// Close the session and release the connection.
    async fn close(&mut self) -> Result<(), ProtocolError>;
}

// ===========================================================================
// Gen-task — submit / poll / fetch (image / video). NOT an event stream.
// ===========================================================================

/// A generation request (image/video). The concrete request shape per dialect
/// (volc-visual, aliyun, …) folds in during Issue 007; this is the spine handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenRequest {
    pub prompt: String,
    /// Free-form, dialect-specific parameters (size, steps, seed, source assets).
    #[serde(default)]
    pub params: Value,
}

/// Opaque handle to a submitted generation job; poll/fetch with it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenHandle {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// Lifecycle state of a generation job.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenStatus {
    Pending,
    Running,
    Done,
    Failed,
}

/// One produced asset (image/video), as a URL or inline bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GenAsset {
    Url {
        url: String,
        media_type: Option<String>,
    },
    Bytes {
        media_type: String,
        data: Bytes,
    },
}

/// A sequence of text segments each carrying a time span — the structured form
/// behind aligned lyrics, ASR transcripts, and TTS word timings. Generic across
/// modalities; consumers render it to LRC / SRT / VTT / a custom UI. Granularity
/// (word vs line) is whatever the provider supplies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimedText {
    pub segments: Vec<TimedSegment>,
}

/// One timed span of text within a [`TimedText`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimedSegment {
    /// Provider-verbatim text; may carry the provider's own markup (e.g. section
    /// tags). The SDK does not clean it — the consumer owns presentation cleanup,
    /// so the protocol stays faithful to what the provider returned.
    pub text: String,
    /// Start offset in seconds from the media's beginning.
    pub start: f64,
    /// End offset in seconds, when the provider supplies one. `None` = unknown
    /// (some sources give only a start; LRC rendering needs only start). A
    /// consumer needing a span can infer it from the next segment's start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
}

/// The completed output of a generation job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenResult {
    pub assets: Vec<GenAsset>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub diagnostic_metadata: Value,
    /// Structured timed text aligned to the primary product (`assets[0]`) —
    /// aligned lyrics, transcript, or word timings. Carries what a
    /// pre-formatted `lrc: String` field could not: the `[(text, start, end)]`
    /// structure the consumer renders to whatever subtitle/karaoke format it
    /// needs, without leaking LRC-the-format or music-the-modality into a type
    /// shared across image/video/music.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timed_text: Option<TimedText>,
}

/// Signed/polled generation capability (image/video), abstracted from
/// `ImageGateway::generate()`. Lifecycle is submit → poll → fetch, **not**
/// `events()` (Issue 002 spec). Implemented by `orchest-provider-visual` and
/// minimax music's REST path in Issue 007.
#[async_trait]
pub trait GenTask: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;

    /// Submit a job and get a handle back.
    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError>;
    /// Poll the job's lifecycle state.
    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError>;
    /// Fetch the produced assets once `Done`.
    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError>;
}

// ===========================================================================
// ASR / TTS / VoiceManager — shapes pinned here; IO converges in Issue 006
// ===========================================================================

/// A spoken/written language tag (BCP-47-ish). The satellite `Language` type
/// folds onto this in Issue 006.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language(pub String);

/// One-shot transcription request. The former rich satellite `TranscribeRequest`
/// has converged onto this (Issue 006); dialect-specific knobs ride in `options`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeRequest {
    pub audio: Bytes,
    pub format: AudioFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default)]
    pub options: Value,
}

/// One-shot transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub diagnostic_metadata: Value,
}

/// Parameters to open a streaming transcription session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTranscribeRequest {
    pub format: AudioFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default)]
    pub options: Value,
}

/// Speech-to-text capability (← `AsrProvider`). The streaming send-side
/// (audio-chunk sink) pairing with `EventStream` is finalized when the
/// providers migrate in Issue 006; the spine pins the surface.
#[async_trait]
pub trait Asr: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;
    fn supported_languages(&self) -> &[Language];

    async fn transcribe(&self, req: TranscribeRequest) -> Result<TranscribeResult, ProtocolError>;
    async fn start_stream(
        &self,
        req: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError>;
}

/// A duplex streaming handle: an audio-chunk sink plus the pulled event stream.
/// The shared shape behind ASR streaming and TTS duplex (and reused by the omni
/// session internals); concrete wiring lands in Issue 006.
#[derive(Debug)]
pub struct RealtimeHandle {
    pub input: tokio::sync::mpsc::Sender<SessionInput>,
    pub events: EventStream,
}

/// Text-to-speech synthesis request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    pub format: AudioFormat,
    #[serde(default)]
    pub options: Value,
}

/// Synthesized audio result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeResult {
    pub audio: Bytes,
    pub format: AudioFormat,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub diagnostic_metadata: Value,
}

/// Text-to-speech capability (← `TtsProvider`). Voice clone/design stays a
/// **separate** optional trait ([`VoiceManager`]), not merged here.
#[async_trait]
pub trait Tts: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;

    async fn synthesize(&self, req: SynthesizeRequest) -> Result<SynthesizeResult, ProtocolError>;
    async fn stream_synthesize(&self, req: SynthesizeRequest)
        -> Result<EventStream, ProtocolError>;
    async fn start_duplex_stream(&self) -> Result<RealtimeHandle, ProtocolError>;
}

/// Optional voice clone/design management, parallel to [`Tts`] (← the existing
/// `VoiceManager`). Only providers that support it (e.g. Minimax) implement it.
#[async_trait]
pub trait VoiceManager: Send + Sync {
    async fn clone_voice(&self, req: Value) -> Result<Value, ProtocolError>;
    async fn design_voice(&self, req: Value) -> Result<Value, ProtocolError>;
    async fn delete_voice(&self, voice_id: &str) -> Result<(), ProtocolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timed_text_serde_round_trip() {
        let tt = TimedText {
            segments: vec![
                TimedSegment {
                    text: "[Verse 1]\n晨光爬上窗台\n".to_string(),
                    start: 11.011,
                    end: Some(16.676),
                },
                TimedSegment {
                    text: "你还在睡".to_string(),
                    start: 16.835,
                    end: None,
                },
            ],
        };
        let wire = serde_json::to_string(&tt).unwrap();
        let restored: TimedText = serde_json::from_str(&wire).unwrap();
        assert_eq!(tt, restored);

        // `end` is omitted from the wire when None and not required on read.
        let v: Value = serde_json::from_str(&wire).unwrap();
        assert!(v["segments"][0].get("end").is_some());
        assert!(v["segments"][1].get("end").is_none());
        let sparse: TimedText = serde_json::from_value(json!({
            "segments": [{ "text": "hi", "start": 0.5 }]
        }))
        .unwrap();
        assert_eq!(sparse.segments[0].end, None);
    }

    #[test]
    fn gen_result_serde_round_trip() {
        let result = GenResult {
            assets: vec![GenAsset::Url {
                url: "https://example.com/a.mp3".to_string(),
                media_type: Some("audio/mpeg".to_string()),
            }],
            diagnostic_metadata: json!({ "provider": "suno" }),
            timed_text: Some(TimedText {
                segments: vec![TimedSegment {
                    text: "la".to_string(),
                    start: 1.0,
                    end: Some(2.0),
                }],
            }),
        };
        let wire = serde_json::to_string(&result).unwrap();
        let restored: GenResult = serde_json::from_str(&wire).unwrap();
        assert_eq!(result, restored);
    }

    #[test]
    fn gen_result_omits_absent_fields_from_wire() {
        // skip_serializing_if keeps the wire shape of a producer that never
        // sets timed_text / diagnostic_metadata identical to before the field
        // existed — and serde(default) lets old payloads still deserialize.
        let result = GenResult {
            assets: vec![],
            diagnostic_metadata: Value::Null,
            timed_text: None,
        };
        let wire = serde_json::to_string(&result).unwrap();
        let v: Value = serde_json::from_str(&wire).unwrap();
        assert!(v.get("timed_text").is_none());
        assert!(v.get("diagnostic_metadata").is_none());

        let minimal: GenResult = serde_json::from_value(json!({ "assets": [] })).unwrap();
        assert_eq!(minimal.timed_text, None);
        assert_eq!(minimal.diagnostic_metadata, Value::Null);
    }
}
