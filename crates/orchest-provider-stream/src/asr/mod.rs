//! WebSocket-backed ASR dialects (Issue 006).
//!
//! The Volcengine openspeech ASR wire layer is built on the shared
//! [`crate::openspeech`] header/constants/gzip core. The `Asr` trait impl + the
//! streaming audio→event loop sit on top of this codec and land alongside the
//! live transport; the per-vendor streaming dialects (deepgram/soniox/…) follow.

pub mod aliyun;
pub mod deepgram;
pub mod elevenlabs;
pub mod soniox;
pub mod volcengine;

use orchest_protocol::{CapabilityExt, ErrorCode, ProtocolError, StreamEvent};

/// Key of the `CapabilityExt::Asr` object that says whether the dialect honours
/// `SessionInput::Finalize` (ADR-0006 D8 `client_finalize`); read it back with
/// `CapabilityDescriptor::client_finalize`.
pub const CLIENT_FINALIZE_KEY: &str = CapabilityExt::ASR_CLIENT_FINALIZE;

/// The ASR descriptor extension every streaming dialect declares.
pub(crate) fn asr_ext(client_finalize: bool) -> CapabilityExt {
    CapabilityExt::Asr(serde_json::json!({ CLIENT_FINALIZE_KEY: client_finalize }))
}

/// The non-fatal answer to a `SessionInput::Finalize` the dialect cannot
/// honour natively (ADR-0006 D7.2): no silent downgrade, no local Turn.
pub(crate) fn finalize_unsupported(dialect: &str) -> StreamEvent {
    StreamEvent::Error {
        error: ProtocolError::new(
            ErrorCode::UnsupportedOption,
            format!("{dialect} streaming ASR does not support finalize"),
        )
        .with_provider(dialect),
        fatal: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dialect_declares_client_finalize() {
        for (descriptor, expected) in [
            (aliyun::entry_descriptor(), aliyun::CLIENT_FINALIZE),
            (deepgram::entry_descriptor(), deepgram::CLIENT_FINALIZE),
            (elevenlabs::entry_descriptor(), elevenlabs::CLIENT_FINALIZE),
            (soniox::entry_descriptor(), soniox::CLIENT_FINALIZE),
            (volcengine::entry_descriptor(), volcengine::CLIENT_FINALIZE),
        ] {
            match descriptor.ext {
                CapabilityExt::Asr(ref value) => {
                    assert_eq!(value[CLIENT_FINALIZE_KEY], serde_json::json!(expected));
                }
                ref other => panic!("{}: expected Asr ext, got {other:?}", descriptor.provider),
            }
            assert_eq!(descriptor.client_finalize(), expected);
        }
        const {
            assert!(!aliyun::CLIENT_FINALIZE && !volcengine::CLIENT_FINALIZE);
            assert!(
                deepgram::CLIENT_FINALIZE && elevenlabs::CLIENT_FINALIZE && soniox::CLIENT_FINALIZE
            );
        }
    }
}
