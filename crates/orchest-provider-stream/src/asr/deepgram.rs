//! Deepgram streaming-ASR result codec (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/deepgram/mod.rs` parsing, now over
//! the spine [`ProtocolError`] / [`StreamEvent`].
//!
//! Deepgram's `v1/listen` WebSocket takes audio as **binary** frames and a few
//! **text** control frames (`Finalize` / `CloseStream`), and returns `Results` /
//! `Error` / lifecycle messages as **text JSON**. This module owns the inbound
//! JSON → unified-event projection; the live `Asr` impl (which needs a transport
//! that carries both binary audio and text control) wraps it once the transport
//! is generalized.

use orchest_protocol::{
    ErrorCode, LifecycleEvent, ProtocolError, StreamEvent, TranscriptStability,
};
use serde::Deserialize;

/// A decoded Deepgram inbound message.
#[derive(Debug, PartialEq)]
pub enum DeepgramMessage {
    Result(DeepgramResult),
    Error(String),
    /// Metadata / UtteranceEnd / SpeechStarted / KeepAlive / unknown — no content.
    Ignored,
}

/// The fields of a `Results` message the spine cares about.
#[derive(Debug, PartialEq)]
pub struct DeepgramResult {
    pub transcript: String,
    /// Deepgram `is_final`: this interim segment is now stable (committed).
    pub is_final: bool,
    /// Deepgram `speech_final`: an endpoint (end of utterance) was detected.
    pub speech_final: bool,
    pub confidence: Option<f64>,
}

/// Parse one Deepgram text message. `Results` → [`DeepgramResult`]; `Error` →
/// its description; everything else is [`DeepgramMessage::Ignored`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_message(text: &str) -> Result<DeepgramMessage, ProtocolError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("failed to parse Deepgram message: {e}"),
        )
    })?;
    match value.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "Results" => Ok(DeepgramMessage::Result(parse_results(value)?)),
        "Error" => Ok(DeepgramMessage::Error(
            value
                .get("description")
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Deepgram stream error")
                .to_string(),
        )),
        _ => Ok(DeepgramMessage::Ignored),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_results(value: serde_json::Value) -> Result<DeepgramResult, ProtocolError> {
    let message: ResultsMessage = serde_json::from_value(value).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("invalid Deepgram Results message: {e}"),
        )
    })?;
    let alternative = message.channel.alternatives.into_iter().next();
    Ok(DeepgramResult {
        transcript: alternative
            .as_ref()
            .map(|a| a.transcript.clone())
            .unwrap_or_default(),
        is_final: message.is_final,
        speech_final: message.speech_final,
        confidence: alternative.and_then(|a| a.confidence),
    })
}

/// Project a Deepgram result onto unified events: a non-empty transcript becomes
/// a `Transcript` (`Committed` once `is_final`, else `Provisional`); a detected
/// endpoint (`speech_final`) additionally emits `EndOfSpeech`.
pub fn map_result(result: &DeepgramResult) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    if !result.transcript.is_empty() {
        events.push(StreamEvent::Transcript {
            text: result.transcript.clone(),
            stability: if result.is_final {
                TranscriptStability::Committed
            } else {
                TranscriptStability::Provisional
            },
            segment: None,
        });
    }
    if result.speech_final {
        events.push(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: None,
        }));
    }
    events
}

#[derive(Debug, Deserialize)]
struct ResultsMessage {
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    channel: Channel,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    confidence: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn results(transcript: &str, is_final: bool, speech_final: bool) -> String {
        json!({
            "type": "Results",
            "is_final": is_final,
            "speech_final": speech_final,
            "channel": {"alternatives": [{"transcript": transcript, "confidence": 0.9}]}
        })
        .to_string()
    }

    #[test]
    fn parses_interim_and_final_results() {
        let interim = parse_message(&results("hello", false, false)).unwrap();
        assert_eq!(
            interim,
            DeepgramMessage::Result(DeepgramResult {
                transcript: "hello".into(),
                is_final: false,
                speech_final: false,
                confidence: Some(0.9),
            })
        );
        match parse_message(&results("hello world", true, true)).unwrap() {
            DeepgramMessage::Result(r) => {
                assert!(r.is_final && r.speech_final);
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn error_message_is_extracted() {
        let raw = json!({"type": "Error", "description": "bad audio"}).to_string();
        assert_eq!(
            parse_message(&raw).unwrap(),
            DeepgramMessage::Error("bad audio".into())
        );
    }

    #[test]
    fn lifecycle_messages_are_ignored() {
        for ty in ["Metadata", "UtteranceEnd", "SpeechStarted", "KeepAlive"] {
            let raw = json!({"type": ty}).to_string();
            assert_eq!(parse_message(&raw).unwrap(), DeepgramMessage::Ignored);
        }
    }

    #[test]
    fn map_result_emits_transcript_with_stability() {
        let events = map_result(&DeepgramResult {
            transcript: "hi".into(),
            is_final: false,
            speech_final: false,
            confidence: None,
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                ..
            }
        ));
    }

    #[test]
    fn map_result_final_with_endpoint_emits_committed_and_end_of_speech() {
        let events = map_result(&DeepgramResult {
            transcript: "done".into(),
            is_final: true,
            speech_final: true,
            confidence: Some(0.8),
        });
        assert!(matches!(
            events[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ));
    }

    #[test]
    fn empty_transcript_emits_nothing() {
        assert!(map_result(&DeepgramResult {
            transcript: String::new(),
            is_final: true,
            speech_final: false,
            confidence: None,
        })
        .is_empty());
    }
}
