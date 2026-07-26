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

/// A generation request (image/video/music). The concrete request shape per
/// dialect (volc-visual, aliyun, …) folds in during Issue 007; this is the
/// spine handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenRequest {
    pub prompt: String,
    /// Free-form, dialect-specific parameters (size, steps, seed, source
    /// assets). Remains the escape hatch for dialect-specific extras (e.g.
    /// Suno's `personaId`); music-modality knobs should go through
    /// [`GenRequest::music`] instead — providers warn on `params` keys they
    /// do not consume.
    #[serde(default)]
    pub params: Value,
    /// Typed music-generation knobs (v0.15, issue 001). Wire-compatible:
    /// `serde(default)` lets pre-existing payloads deserialize, and the field
    /// is omitted from the wire when unset, so Py/Node wire passthrough is
    /// unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music: Option<MusicParams>,
}

/// Vocal gender for music generation.
///
/// A typed enum rather than a free-form `String` because the accepted value
/// set is closed and binary across the music dialects: Suno's `vocalGender`
/// takes exactly `"m"`/`"f"` (the sunoapi.org proxy this SDK targets — Suno
/// has no official public API; see
/// `crates/orchest-provider-http/src/gen/suno.rs`), and Aliyun fun-music's
/// `gender` takes `"male"`/`"female"` (docs/external/aliyun/
/// music-generation.md). Variants serialize to the Suno spelling (`"m"` /
/// `"f"`); providers with a different spelling translate at their submit
/// boundary. An unknown wire value fails deserialization loudly instead of
/// being silently dropped.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VocalGender {
    #[serde(rename = "m")]
    Male,
    #[serde(rename = "f")]
    Female,
}

/// Typed music-modality knobs for [`GenRequest`] (v0.15, issue 001) — the
/// structured alternative to cherry-picking keys out of the free-form
/// [`GenRequest::params`], where a misspelled or unsupported key used to be
/// dropped silently.
///
/// All fields are optional; `None` means "not set" and is omitted from the
/// wire. Field names serialize in camelCase to match provider wire
/// conventions (Suno's naming is the canonical set: `negativeTags`,
/// `vocalGender`, `styleWeight`, `weirdnessConstraint`, `audioWeight`).
/// Typed fields take precedence over raw `params` keys at the provider
/// submit boundary; a provider only maps the fields its dialect actually
/// supports and leaves the rest unset.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MusicParams {
    /// Custom-mode lyrics. When set (and non-empty), dialects like Suno
    /// switch to custom mode where the lyrics replace the prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<String>,
    /// Instrumental-only generation (no vocals).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrumental: Option<bool>,
    /// Style description string (genre / tempo / mood / instrumentation …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Track title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Styles/elements to avoid (Suno `negativeTags`).
    #[serde(
        default,
        rename = "negativeTags",
        skip_serializing_if = "Option::is_none"
    )]
    pub negative_tags: Option<String>,
    /// Vocal gender (Suno `vocalGender`). See [`VocalGender`] for the
    /// enum-over-String decision.
    #[serde(
        default,
        rename = "vocalGender",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_gender: Option<VocalGender>,
    /// Strength of the style guidance, 0–1 (Suno `styleWeight`).
    #[serde(
        default,
        rename = "styleWeight",
        skip_serializing_if = "Option::is_none"
    )]
    pub style_weight: Option<f64>,
    /// How unusual/experimental the output may be, 0–1 (Suno
    /// `weirdnessConstraint`).
    #[serde(
        default,
        rename = "weirdnessConstraint",
        skip_serializing_if = "Option::is_none"
    )]
    pub weirdness_constraint: Option<f64>,
    /// Weight of the audio influence vs. the style, 0–1 (Suno `audioWeight`).
    #[serde(
        default,
        rename = "audioWeight",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio_weight: Option<f64>,
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

    #[test]
    fn music_params_serde_uses_provider_camel_case_names() {
        let music = MusicParams {
            lyrics: Some("la la".to_string()),
            instrumental: Some(true),
            style: Some("lofi".to_string()),
            title: Some("my song".to_string()),
            negative_tags: Some("no choir".to_string()),
            vocal_gender: Some(VocalGender::Female),
            style_weight: Some(0.5),
            weirdness_constraint: Some(0.2),
            audio_weight: Some(0.8),
        };
        let v = serde_json::to_value(&music).unwrap();
        // Wire names match the provider (Suno) conventions.
        assert_eq!(v["negativeTags"], "no choir");
        assert_eq!(v["vocalGender"], "f");
        assert_eq!(v["styleWeight"], 0.5);
        assert_eq!(v["weirdnessConstraint"], 0.2);
        assert_eq!(v["audioWeight"], 0.8);
        assert_eq!(v["lyrics"], "la la");
        assert_eq!(v["instrumental"], true);
        assert!(v.get("negative_tags").is_none());
        assert!(v.get("vocal_gender").is_none());
        // Round trip preserves everything.
        let restored: MusicParams = serde_json::from_value(v).unwrap();
        assert_eq!(restored, music);
    }

    #[test]
    fn music_params_omits_unset_fields_from_wire() {
        let v = serde_json::to_value(&MusicParams::default()).unwrap();
        assert_eq!(v, json!({}));
        let sparse: MusicParams = serde_json::from_value(json!({ "style": "lofi" })).unwrap();
        assert_eq!(sparse.style.as_deref(), Some("lofi"));
        assert_eq!(sparse.vocal_gender, None);
    }

    #[test]
    fn vocal_gender_serializes_to_suno_spelling() {
        assert_eq!(serde_json::to_value(VocalGender::Male).unwrap(), "m");
        assert_eq!(serde_json::to_value(VocalGender::Female).unwrap(), "f");
        // An unknown wire value is a loud deserialization error, not a
        // silently dropped knob.
        assert!(serde_json::from_value::<VocalGender>(json!("x")).is_err());
    }

    #[test]
    fn gen_request_music_field_is_wire_compatible() {
        // Pre-existing payloads (no `music` key) still deserialize.
        let old: GenRequest = serde_json::from_value(json!({
            "prompt": "a fox",
            "params": { "size": "1024*1024" }
        }))
        .unwrap();
        assert_eq!(old.music, None);

        // A request with music unset serializes identically to before the
        // field existed — no `music` key on the wire.
        let req = GenRequest {
            prompt: "a fox".to_string(),
            params: json!({}),
            music: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert!(v.get("music").is_none());

        // And with music set it round-trips.
        let req = GenRequest {
            music: Some(MusicParams {
                style: Some("lofi".to_string()),
                ..MusicParams::default()
            }),
            ..req
        };
        let restored: GenRequest =
            serde_json::from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(restored.music, req.music);
    }
}
