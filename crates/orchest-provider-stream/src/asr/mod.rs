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

use std::sync::Arc;

use orchest_protocol::{
    CapabilityExt, ErrorCode, LifecycleEvent, ProtocolError, StreamEvent, UpstreamErrorDetail,
};
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
/// Metadata already on the error is kept.
pub(crate) fn finalize_error(error: ProtocolError) -> StreamEvent {
    let mut error = error;
    match error.diagnostic_metadata.as_object_mut() {
        Some(metadata) => {
            metadata.insert("operation".into(), json!(FINALIZE_OPERATION));
        }
        None => error.diagnostic_metadata = json!({ "operation": FINALIZE_OPERATION }),
    }
    StreamEvent::Error {
        error,
        fatal: false,
    }
}

/// An error message a streaming ASR upstream sent: its own error code, when it
/// has one, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamError {
    pub code: Option<String>,
    pub message: String,
}

impl UpstreamError {
    pub fn new(code: Option<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The fatal event for this error: the upstreams close the socket after
    /// sending one. The upstream code is kept in `ProtocolError.upstream`.
    pub(crate) fn into_event(self, dialect: &str) -> StreamEvent {
        let mut error = ProtocolError::new(ErrorCode::ProviderTaskFailed, self.message.clone())
            .with_provider(dialect);
        error.upstream = Some(Arc::new(UpstreamErrorDetail {
            code: self.code,
            message: Some(self.message),
            body: None,
        }));
        StreamEvent::Error { error, fatal: true }
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
///
/// Only the dialect can tell which upstream message answers a `Finalize`
/// (Deepgram `from_finalize`, Soniox `<fin>`, the ElevenLabs commit), so a
/// VAD Turn never counts as the answer. A request left unanswered for
/// [`FINALIZE_TIMEOUT`] is answered with a non-fatal `Timeout` instead. If
/// the upstream answer arrives after that, it is not a second result: its
/// segment-less Turn is dropped, and a Turn that ends Stable text is kept as an
/// ordinary Turn, since every Stable must be ended by one.
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

    /// Pass on the events the dialect mapped from one upstream message;
    /// `answers_finalize` says whether that message is the upstream answer to
    /// a `Finalize`. It settles the oldest request, or, when none is left
    /// (it timed out), the segment-less Turn it carries is dropped.
    pub(crate) fn settle(
        &mut self,
        answers_finalize: bool,
        mut events: Vec<StreamEvent>,
    ) -> Vec<StreamEvent> {
        if answers_finalize && self.deadlines.pop_front().is_none() {
            events.retain(|event| {
                !matches!(
                    event,
                    StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { segment: None })
                )
            });
        }
        events
    }

    /// Pass on the events of an upstream error message. With a `Finalize`
    /// pending, the error is also its answer: a non-fatal copy tagged
    /// `operation = "finalize"` goes first, then the fatal error that ends
    /// the session (the upstreams close the socket after an error).
    pub(crate) fn upstream_failed(&mut self, mut events: Vec<StreamEvent>) -> Vec<StreamEvent> {
        if !self.is_pending() {
            return events;
        }
        let answer = events.iter().find_map(|event| match event {
            StreamEvent::Error { error, .. } => Some(finalize_error(error.clone())),
            _ => None,
        });
        if let Some(answer) = answer {
            self.deadlines.pop_front();
            events.insert(0, answer);
        }
        events
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
        let bare_turn = || StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { segment: None });
        watch.sent();
        watch.sent();
        // a VAD Turn is passed on and answers nothing
        assert_eq!(watch.settle(false, vec![bare_turn()]).len(), 1);
        assert!(watch.is_pending());
        assert_eq!(watch.settle(true, vec![bare_turn()]).len(), 1);
        assert!(watch.is_pending());
        watch.expired().await;
        assert_finalize_error(&watch.expire("d"), ErrorCode::Timeout);
        assert!(!watch.is_pending());
        // the late upstream answer is not a second result
        assert!(watch.settle(true, vec![bare_turn()]).is_empty());
        let ending_stable = StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: Some(orchest_protocol::SegmentRef {
                segment_id: Some("final0".into()),
                update_kind: orchest_protocol::TranscriptUpdateKind::Append,
            }),
        });
        assert_eq!(watch.settle(true, vec![ending_stable]).len(), 1);
    }

    #[test]
    fn an_upstream_error_answers_a_pending_finalize_before_ending_the_session() {
        let mut watch = FinalizeWatch::default();
        let fatal = || {
            UpstreamError::new(Some("commit_throttled".into()), "too many commits")
                .into_event("elevenlabs")
        };
        // nothing pending: only the fatal error
        assert_eq!(watch.upstream_failed(vec![fatal()]).len(), 1);
        watch.sent();
        let events = watch.upstream_failed(vec![fatal()]);
        assert!(!watch.is_pending());
        assert_eq!(events.len(), 2);
        assert_finalize_error(&events[0], ErrorCode::ProviderTaskFailed);
        let StreamEvent::Error { error, fatal } = &events[1] else {
            panic!("expected the fatal error, got {:?}", events[1]);
        };
        assert!(fatal);
        let upstream = error.upstream.as_ref().expect("upstream detail");
        assert_eq!(upstream.code.as_deref(), Some("commit_throttled"));
        assert_eq!(error.provider.as_deref(), Some("elevenlabs"));
    }

    #[test]
    fn finalize_error_keeps_existing_metadata() {
        let mut error = ProtocolError::new(ErrorCode::Timeout, "x");
        error.diagnostic_metadata = json!({ "attempt": 2 });
        let StreamEvent::Error { error, .. } = finalize_error(error) else {
            unreachable!()
        };
        assert_eq!(error.diagnostic_metadata["attempt"], 2);
        assert_eq!(error.diagnostic_metadata["operation"], FINALIZE_OPERATION);
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
