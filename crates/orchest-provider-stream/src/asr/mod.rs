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

use std::collections::VecDeque;
use std::time::Duration;

use orchest_protocol::{CapabilityExt, ErrorCode, LifecycleEvent, ProtocolError, StreamEvent};
use serde_json::json;
use tokio::time::Instant;

/// Key of the `CapabilityExt::Asr` object that says whether the dialect honours
/// `SessionInput::Finalize` (ADR-0006 D8 `client_finalize`); read it back with
/// `CapabilityDescriptor::client_finalize`.
pub const CLIENT_FINALIZE_KEY: &str = CapabilityExt::ASR_CLIENT_FINALIZE;

/// The ASR descriptor extension every streaming dialect declares.
pub(crate) fn asr_ext(client_finalize: bool) -> CapabilityExt {
    CapabilityExt::Asr(json!({ CLIENT_FINALIZE_KEY: client_finalize }))
}

/// `diagnostic_metadata.operation` on the error that answers a
/// `SessionInput::Finalize`, so a caller can tell it from other stream errors.
pub const FINALIZE_OPERATION: &str = "finalize";

/// How long a dialect waits for the Turn that answers a `Finalize` before it
/// reports the finalize as timed out. The session stays open either way.
pub const FINALIZE_TIMEOUT: Duration = Duration::from_secs(10);

/// Tag `error` as the answer to a `Finalize` and wrap it as the non-fatal
/// event the Finalize receipt requires (ADR-0006 D7.2): the session stays open.
pub(crate) fn finalize_error(error: ProtocolError) -> StreamEvent {
    let mut error = error;
    error.diagnostic_metadata = json!({ "operation": FINALIZE_OPERATION });
    StreamEvent::Error {
        error,
        fatal: false,
    }
}

/// The answer to a `SessionInput::Finalize` the dialect cannot honour
/// natively: no silent downgrade, no local Turn.
pub(crate) fn finalize_unsupported(dialect: &str) -> StreamEvent {
    finalize_error(
        ProtocolError::new(
            ErrorCode::UnsupportedOption,
            format!("{dialect} streaming ASR does not support finalize"),
        )
        .with_provider(dialect),
    )
}

/// The `Finalize` requests a dialect has sent upstream and not yet answered.
/// The first Turn after a `Finalize` answers it; a request left unanswered for
/// [`FINALIZE_TIMEOUT`] is answered with a non-fatal `Timeout` instead, so
/// every `Finalize` gets exactly one result.
#[derive(Debug, Default)]
pub(crate) struct FinalizeWatch {
    deadlines: VecDeque<Instant>,
}

impl FinalizeWatch {
    pub(crate) fn sent(&mut self) {
        self.deadlines.push_back(Instant::now() + FINALIZE_TIMEOUT);
    }

    pub(crate) fn is_pending(&self) -> bool {
        !self.deadlines.is_empty()
    }

    /// Note an event the dialect emitted; a Turn answers the oldest request.
    pub(crate) fn observe(&mut self, event: &StreamEvent) {
        if matches!(
            event,
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ) {
            self.deadlines.pop_front();
        }
    }

    /// Resolves when the oldest request times out; never while none is pending.
    pub(crate) async fn expired(&self) {
        match self.deadlines.front() {
            Some(deadline) => tokio::time::sleep_until(*deadline).await,
            None => std::future::pending().await,
        }
    }

    /// Answer the oldest request with a timeout.
    pub(crate) fn expire(&mut self, dialect: &str) -> StreamEvent {
        self.deadlines.pop_front();
        finalize_error(
            ProtocolError::new(
                ErrorCode::Timeout,
                format!(
                    "{dialect} did not answer finalize within {}s",
                    FINALIZE_TIMEOUT.as_secs()
                ),
            )
            .with_provider(dialect),
        )
    }
}

/// Assert `event` is the non-fatal Finalize answer carrying `code`.
#[cfg(test)]
pub(crate) fn assert_finalize_error(event: &StreamEvent, code: ErrorCode) {
    match event {
        StreamEvent::Error { error, fatal } => {
            assert_eq!(error.code, code);
            assert!(!fatal, "a finalize error must not close the session");
            assert_eq!(error.diagnostic_metadata["operation"], FINALIZE_OPERATION);
        }
        other => panic!("expected a {code:?} finalize error, got {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn finalize_watch_answers_each_request_once() {
        let mut watch = FinalizeWatch::default();
        assert!(!watch.is_pending());
        watch.sent();
        watch.sent();
        watch.observe(&StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: None,
        }));
        assert!(watch.is_pending());
        watch.expired().await;
        assert_finalize_error(&watch.expire("d"), ErrorCode::Timeout);
        assert!(!watch.is_pending());
        // a Turn with nothing pending is an ordinary VAD Turn
        watch.observe(&StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: None,
        }));
        assert!(!watch.is_pending());
    }

    #[test]
    fn finalize_unsupported_is_tagged_and_non_fatal() {
        assert_finalize_error(&finalize_unsupported("d"), ErrorCode::UnsupportedOption);
    }

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
