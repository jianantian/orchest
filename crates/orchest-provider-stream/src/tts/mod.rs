//! WebSocket-backed TTS dialects (Issue 006).
//!
//! The Volcengine openspeech TTS wire layer is built on the shared
//! [`crate::openspeech`] core, using the event-framing variant: `FLAG_WITH_EVENT`
//! with an `event` number and an optional `session_id`. The `Tts` trait impl and
//! the synthesize/duplex loops sit on top of this codec and land alongside the
//! live transport; minimax-ws TTS follows.

pub mod aliyun;
pub mod minimax;
pub mod volcengine;

use orchest_protocol::{ErrorCode, ProtocolError, SynthesizeRequest};

/// Resolve a required non-empty voice from [`SynthesizeRequest`].
///
/// Live stream dialects (aliyun / minimax / volcengine) do not apply a provider
/// default: `voice: None` or `Some("")` means the caller omitted the field and
/// must be rejected with [`ErrorCode::InvalidRequest`] before any provider I/O.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub(crate) fn require_explicit_voice(request: &SynthesizeRequest) -> Result<&str, ProtocolError> {
    match request.voice.as_deref() {
        Some(voice) if !voice.is_empty() => Ok(voice),
        _ => Err(ProtocolError::new(
            ErrorCode::InvalidRequest,
            "SynthesizeRequest.voice is required for this TTS dialect; None/empty is not a provider default — supply an explicit voice id",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::AudioFormat;
    use serde_json::Value;

    fn req(voice: Option<&str>) -> SynthesizeRequest {
        SynthesizeRequest {
            text: "hi".into(),
            voice: voice.map(str::to_string),
            format: AudioFormat::Mp3,
            options: Value::Null,
        }
    }

    #[test]
    fn require_explicit_voice_accepts_non_empty() {
        assert_eq!(
            require_explicit_voice(&req(Some("longxiaochun"))).unwrap(),
            "longxiaochun"
        );
    }

    #[test]
    fn require_explicit_voice_rejects_none_and_empty() {
        for voice in [None, Some("")] {
            let err = require_explicit_voice(&req(voice)).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidRequest);
            assert!(
                err.message.contains("voice"),
                "expected voice-focused message, got {}",
                err.message
            );
        }
    }
}
