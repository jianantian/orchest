//! Soniox streaming-ASR token codec (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/soniox/mod.rs` parsing, over the
//! spine [`ProtocolError`] / [`StreamEvent`].
//!
//! Soniox streams **tokens**: each carries `text` and an `is_final` flag, so one
//! message mixes a committed prefix (final tokens) and a revisable tail
//! (non-final tokens). `finished` marks end of stream. Audio out is binary; this
//! module owns the inbound JSON → unified-event projection (the live `Asr` impl
//! wraps it once the transport carries both binary audio and text frames).

use orchest_protocol::{
    ErrorCode, LifecycleEvent, ProtocolError, StreamEvent, TranscriptStability,
};
use serde::Deserialize;

/// A decoded Soniox inbound message.
#[derive(Debug, PartialEq)]
pub enum SonioxMessage {
    Tokens {
        /// Concatenated `is_final` tokens (committed).
        final_text: String,
        /// Concatenated non-final tokens (revisable).
        provisional_text: String,
        /// End of stream.
        finished: bool,
    },
    Error(String),
    /// No tokens and not finished — nothing to surface.
    Ignored,
}

/// Parse one Soniox text message into [`SonioxMessage`]. An `error_code` yields
/// `Error`; a message with no tokens that is not `finished` is `Ignored`.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_message(text: &str) -> Result<SonioxMessage, ProtocolError> {
    let message: RawMessage = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("failed to parse Soniox message: {e}"),
        )
    })?;
    if let Some(error_code) = message.error_code {
        return Ok(SonioxMessage::Error(format!(
            "soniox error {error_code}: {}",
            message
                .error_message
                .unwrap_or_else(|| "stream error".into())
        )));
    }
    if message.tokens.is_empty() && !message.finished {
        return Ok(SonioxMessage::Ignored);
    }
    let mut final_text = String::new();
    let mut provisional_text = String::new();
    for token in message.tokens {
        if token.is_final {
            final_text.push_str(&token.text);
        } else {
            provisional_text.push_str(&token.text);
        }
    }
    Ok(SonioxMessage::Tokens {
        final_text,
        provisional_text,
        finished: message.finished,
    })
}

/// Project a Soniox message onto unified events: committed tokens → a
/// `Committed` `Transcript`, the revisable tail → a `Provisional` `Transcript`,
/// and `finished` → `EndOfSpeech`. An `Error` becomes a single fatal `Error`.
pub fn map_message(message: SonioxMessage) -> Vec<StreamEvent> {
    match message {
        SonioxMessage::Tokens {
            final_text,
            provisional_text,
            finished,
        } => {
            let mut events = Vec::new();
            if !final_text.is_empty() {
                events.push(StreamEvent::Transcript {
                    text: final_text,
                    stability: TranscriptStability::Committed,
                    segment: None,
                });
            }
            if !provisional_text.is_empty() {
                events.push(StreamEvent::Transcript {
                    text: provisional_text,
                    stability: TranscriptStability::Provisional,
                    segment: None,
                });
            }
            if finished {
                events.push(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                    segment: None,
                }));
            }
            events
        }
        SonioxMessage::Error(message) => vec![StreamEvent::Error {
            error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
            fatal: true,
        }],
        SonioxMessage::Ignored => Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(default)]
    tokens: Vec<Token>,
    #[serde(default)]
    finished: bool,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Token {
    text: String,
    #[serde(default)]
    is_final: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tokens_msg(tokens: &[(&str, bool)], finished: bool) -> String {
        let tokens: Vec<_> = tokens
            .iter()
            .map(|(t, f)| json!({"text": t, "is_final": f}))
            .collect();
        json!({"tokens": tokens, "finished": finished}).to_string()
    }

    #[test]
    fn parses_mixed_final_and_provisional_tokens() {
        let raw = tokens_msg(&[("hello ", true), ("wor", false)], false);
        assert_eq!(
            parse_message(&raw).unwrap(),
            SonioxMessage::Tokens {
                final_text: "hello ".into(),
                provisional_text: "wor".into(),
                finished: false,
            }
        );
    }

    #[test]
    fn empty_unfinished_message_is_ignored() {
        assert_eq!(
            parse_message(&json!({"tokens": []}).to_string()).unwrap(),
            SonioxMessage::Ignored
        );
    }

    #[test]
    fn error_code_becomes_error() {
        let raw = json!({"error_code": "401", "error_message": "bad key"}).to_string();
        match parse_message(&raw).unwrap() {
            SonioxMessage::Error(m) => assert!(m.contains("401") && m.contains("bad key")),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn map_emits_committed_provisional_and_end_of_speech() {
        let events = map_message(SonioxMessage::Tokens {
            final_text: "hello".into(),
            provisional_text: "world".into(),
            finished: true,
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
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ));
    }

    #[test]
    fn map_error_is_fatal() {
        let events = map_message(SonioxMessage::Error("boom".into()));
        assert!(matches!(events[0], StreamEvent::Error { fatal: true, .. }));
    }
}
