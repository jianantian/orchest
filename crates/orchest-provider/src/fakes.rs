//! Deterministic in-process `Asr`/`Tts` fakes for downstream offline tests and
//! demos (issue #196: no reusable fake existed anywhere in the workspace,
//! forcing every downstream crate to hand-write its own `orchest_protocol`
//! trait impl). Gated behind the `testing` feature — no impl crate, no
//! network deps, never pulled into a production build by accident.
//!
//! Shape lifted from `examples/demo/briefing-desk/src/media.rs`'s original
//! `FakeAsr`/`FakeTts`, generalized so the transcript/audio marker are
//! injectable instead of hard-coded to that demo's fixtures.

use async_trait::async_trait;
use orchest_protocol::{
    Asr, CapabilityDescriptor, ErrorCode, EventStream, Language, ProtocolError, RealtimeHandle,
    StreamingTranscribeRequest, SynthesizeRequest, SynthesizeResult, TranscribeRequest,
    TranscribeResult, Tts,
};

/// Deterministic offline `Asr`: always returns the same fixed transcript,
/// regardless of the audio bytes it's given. Streaming is unsupported.
#[derive(Debug, Clone)]
pub struct FakeAsr {
    transcript: String,
}

impl FakeAsr {
    /// A fake ASR that always transcribes to `transcript`.
    pub fn new(transcript: impl Into<String>) -> Self {
        Self {
            transcript: transcript.into(),
        }
    }
}

impl Default for FakeAsr {
    /// A fake ASR with a generic placeholder transcript. Prefer
    /// [`FakeAsr::new`] when the test/demo asserts on the transcript content.
    fn default() -> Self {
        Self::new("This is a fake transcript.")
    }
}

#[async_trait]
impl Asr for FakeAsr {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "fake-asr"
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("fake", "fake-asr", orchest_protocol::Capability::Asr)
            .with_input_modalities([orchest_protocol::Modality::Audio])
            .with_output_modalities([orchest_protocol::Modality::Text])
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, req: TranscribeRequest) -> Result<TranscribeResult, ProtocolError> {
        Ok(TranscribeResult {
            text: self.transcript.clone(),
            language: None,
            diagnostic_metadata: serde_json::json!({"fake": true, "input_bytes": req.audio.len()}),
        })
    }

    async fn start_stream(
        &self,
        _req: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "FakeAsr does not support streaming",
        ))
    }
}

/// Deterministic offline `Tts`: synthesizes a fixed marker payload derived
/// from the input text (not real audio), so callers can assert on it without
/// a network round-trip. Streaming/duplex are unsupported.
///
/// Unlike live stream dialects, [`SynthesizeRequest::voice`] may be `None`
/// here — the fake ignores voice entirely.
#[derive(Debug, Clone)]
pub struct FakeTts {
    marker_prefix: String,
}

impl FakeTts {
    /// A fake TTS whose synthesized bytes start with `marker_prefix`,
    /// followed by a deterministic summary of the input text.
    pub fn new(marker_prefix: impl Into<String>) -> Self {
        Self {
            marker_prefix: marker_prefix.into(),
        }
    }
}

impl Default for FakeTts {
    fn default() -> Self {
        Self::new("FAKE AUDIO (fake TTS)")
    }
}

#[async_trait]
impl Tts for FakeTts {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "fake-tts"
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("fake", "fake-tts", orchest_protocol::Capability::Tts)
            .with_input_modalities([orchest_protocol::Modality::Text])
            .with_output_modalities([orchest_protocol::Modality::Audio])
    }

    async fn synthesize(&self, req: SynthesizeRequest) -> Result<SynthesizeResult, ProtocolError> {
        let marker = format!("{}\ntext_len={}\n", self.marker_prefix, req.text.len());
        Ok(SynthesizeResult {
            audio: bytes::Bytes::from(marker.into_bytes()),
            format: req.format,
            diagnostic_metadata: serde_json::json!({"fake": true}),
        })
    }

    async fn stream_synthesize(
        &self,
        _req: SynthesizeRequest,
    ) -> Result<EventStream, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "FakeTts does not support streaming",
        ))
    }

    async fn start_duplex_stream(&self) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "FakeTts does not support duplex",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::AudioFormat;

    #[tokio::test]
    async fn fake_asr_transcribes_deterministically_to_injected_transcript() {
        let asr = FakeAsr::new("hello world");
        let result = asr
            .transcribe(TranscribeRequest {
                audio: bytes::Bytes::from_static(b"anything"),
                format: AudioFormat::Wav,
                language: None,
                options: serde_json::Value::Null,
            })
            .await
            .unwrap();
        assert_eq!(result.text, "hello world");
    }

    #[tokio::test]
    async fn fake_asr_default_is_deterministic() {
        let asr = FakeAsr::default();
        let result = asr
            .transcribe(TranscribeRequest {
                audio: bytes::Bytes::from_static(b"anything"),
                format: AudioFormat::Wav,
                language: None,
                options: serde_json::Value::Null,
            })
            .await
            .unwrap();
        assert_eq!(result.text, "This is a fake transcript.");
    }

    #[tokio::test]
    async fn fake_tts_synthesizes_deterministic_marker() {
        let tts = FakeTts::default();
        let result = tts
            .synthesize(SynthesizeRequest {
                text: "hi".to_string(),
                voice: None,
                format: AudioFormat::Wav,
                options: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let audio_str = String::from_utf8(result.audio.to_vec()).unwrap();
        assert!(audio_str.starts_with("FAKE AUDIO (fake TTS)"));
        assert!(audio_str.contains("text_len=2"));
    }

    #[tokio::test]
    async fn fake_asr_start_stream_is_unsupported() {
        let asr = FakeAsr::default();
        let err = asr
            .start_stream(StreamingTranscribeRequest {
                format: AudioFormat::Wav,
                language: None,
                options: serde_json::Value::Null,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedOperation);
    }

    #[tokio::test]
    async fn fake_tts_stream_and_duplex_are_unsupported() {
        let tts = FakeTts::default();
        let err = tts
            .stream_synthesize(SynthesizeRequest {
                text: "hi".to_string(),
                voice: None,
                format: AudioFormat::Wav,
                options: serde_json::Value::Null,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedOperation);

        let err = tts.start_duplex_stream().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedOperation);
    }
}
