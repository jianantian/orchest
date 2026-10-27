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

use orchest_protocol::{ErrorCode, ProtocolError, StreamEvent};

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
