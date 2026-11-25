use std::cell::Cell;
use std::future::Future;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    AttemptOutcome, CapturedField, EvidenceError, EvidenceField, EvidenceKind, EvidenceRecord,
    EvidenceSession, RunEvidence,
};
use crate::model::{
    ContentBlock, ModelAdapter, ModelError, ModelResponse, ModelStreamChunk, StopReason,
};
use crate::run::RunId;
use crate::session::{SessionEvent, SessionEventKind};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HookDisposition {
    Accepted,
    Rewritten,
    Rejected,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AttemptPurpose {
    AgentStep,
    CompactionSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdapterFailure {
    RateLimit,
    ServerError,
    Timeout,
    Stream,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct EvidenceEventRef {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub kind: EvidenceEventKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceEventKind {
    AssistantMessage,
    ToolDispatchMarker,
    ToolResult,
    Compacted,
}

impl EvidenceEventRef {
    pub(super) fn from_event(event: &SessionEvent) -> Option<Self> {
        let kind = match event.kind {
            SessionEventKind::AssistantMessage { .. } => EvidenceEventKind::AssistantMessage,
            SessionEventKind::ToolDispatched { .. } => EvidenceEventKind::ToolDispatchMarker,
            SessionEventKind::ToolResult { .. } => EvidenceEventKind::ToolResult,
            SessionEventKind::Compacted { .. } => EvidenceEventKind::Compacted,
            _ => return None,
        };
        Some(Self {
            seq: event.seq,
            timestamp_ms: event.timestamp_ms,
            kind,
        })
    }
}

impl EvidenceSession {
    /// Verify the original event reference with a watermark from the same
    /// worker and check that the event is still in the bound Session. The
    /// caller must supply records obtained from this evidence store.
    pub async fn verify_event(
        &self,
        reference: &EvidenceRecord,
        watermark: &EvidenceRecord,
    ) -> Result<bool, EvidenceError> {
        let event = match reference.kind {
            EvidenceKind::SessionEvent { event, .. }
            | EvidenceKind::CompactionApplied { event, .. } => event,
            _ => return Ok(false),
        };
        let EvidenceKind::SessionDurableThrough { seq } = watermark.kind else {
            return Ok(false);
        };
        if seq < event.seq
            || reference.worker_id != watermark.worker_id
            || reference.run_id != watermark.run_id
            || reference.session_instance != watermark.session_instance
            || reference.session_instance != self.instance().await?
        {
            return Ok(false);
        }
        let stored = self
            .store
            .read_event(&self.session_id, event.seq)
            .await
            .map_err(|_| EvidenceError::SessionUnavailable)?;
        if stored.as_ref().and_then(EvidenceEventRef::from_event) != Some(event) {
            return Ok(false);
        }
        self.database
            .contains(reference.session_instance, [reference, watermark])
            .await
    }
}

#[derive(Clone)]
pub(crate) struct AttemptObservation {
    pub(crate) id: Uuid,
    pub(crate) evidence: RunEvidence,
    pub(crate) run_id: RunId,
    pub(crate) step: u32,
    purpose: AttemptPurpose,
}

impl AttemptObservation {
    pub(crate) fn start(
        evidence: &RunEvidence,
        (run_id, step): (RunId, u32),
        ordinal: u32,
        purpose: AttemptPurpose,
        model: &dyn ModelAdapter,
    ) -> Self {
        let id = Uuid::new_v4();
        evidence.record(
            run_id,
            step,
            EvidenceKind::AttemptStarted {
                attempt_id: id,
                ordinal,
                purpose,
                provider: identity(model.provider_name()),
                model: identity(model.model_name()),
            },
        );
        Self {
            id,
            evidence: evidence.clone(),
            run_id,
            step,
            purpose,
        }
    }

    fn record(&self, kind: EvidenceKind) {
        self.evidence.record(self.run_id, self.step, kind);
    }

    pub(crate) fn finish(&self, outcome: AttemptOutcome) {
        self.record(EvidenceKind::AttemptFinished {
            attempt_id: self.id,
            outcome,
        });
    }

    pub(crate) fn returned(&self, result: &Result<ModelResponse, ModelError>) {
        let Ok(response) = result else {
            let failure = match result.as_ref().err().map(crate::run::retry::classify) {
                Some(crate::run::retry::RetryClass::RateLimit) => AdapterFailure::RateLimit,
                Some(crate::run::retry::RetryClass::ServerError) => AdapterFailure::ServerError,
                Some(crate::run::retry::RetryClass::Timeout) => AdapterFailure::Timeout,
                Some(crate::run::retry::RetryClass::StreamInterrupted) => AdapterFailure::Stream,
                _ => AdapterFailure::Other,
            };
            self.record(EvidenceKind::AdapterFailed {
                attempt_id: self.id,
                failure,
            });
            return;
        };
        let stop_reason = match &response.stop_reason {
            StopReason::Other(_) => None,
            reason => Some(reason.clone()),
        };
        self.record(EvidenceKind::AdapterReturned {
            attempt_id: self.id,
            stop_reason,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            option_adjustments: response.option_adjustments.len(),
        });
        // The summarizer may echo serialized content excluded by its source policy.
        if self.purpose == AttemptPurpose::CompactionSummary {
            return;
        }
        self.content(EvidenceField::Response, &response.content);
        self.evidence.capture_field(
            self.run_id,
            self.step,
            self.id,
            EvidenceField::OptionAdjustments,
            &response.option_adjustments,
            |budget| {
                for adjustment in &response.option_adjustments {
                    budget.text(&adjustment.option)?;
                    budget.json(&adjustment.requested, 0)?;
                    budget.json(&adjustment.applied, 0)?;
                    budget.text(&adjustment.reason)?;
                }
                Ok(())
            },
        );
    }

    pub(crate) fn hook(&self, disposition: HookDisposition) {
        self.record(EvidenceKind::AfterModel {
            attempt_id: self.id,
            disposition,
        });
    }

    pub(crate) fn hook_result(&self, before: &[ContentBlock], after: &[ContentBlock]) {
        let mut budget = super::content::CaptureBudget::new(self.evidence.database.record_bytes());
        let disposition = if before.len() != after.len() {
            HookDisposition::Rewritten
        } else if budget
            .blocks(before)
            .and_then(|()| budget.blocks(after))
            .is_err()
        {
            HookDisposition::Unknown
        } else if before == after {
            HookDisposition::Accepted
        } else {
            HookDisposition::Rewritten
        };
        self.hook(disposition);
    }

    pub(crate) fn content(&self, field: EvidenceField, content: &[ContentBlock]) {
        self.evidence
            .capture_field(self.run_id, self.step, self.id, field, &content, |budget| {
                budget.blocks(content)
            });
    }

    pub(crate) fn event(&self, event: &SessionEvent) {
        if let Some(event) = EvidenceEventRef::from_event(event) {
            self.record(EvidenceKind::SessionEvent {
                attempt_id: self.id,
                event,
            });
        }
    }

    pub(crate) fn observe_stream(&self, chunk: &ModelStreamChunk, index: u64, text_count: u64) {
        if let ModelStreamChunk::Text { delta } = chunk {
            if text_count <= 8 {
                self.evidence.record_with(self.run_id, self.step, || {
                    let capture = self
                        .evidence
                        .content
                        .as_ref()
                        .map(|policy| {
                            policy.capture(
                                EvidenceField::StreamText,
                                delta,
                                self.evidence.database.record_bytes().min(4096),
                                |budget| budget.text(delta),
                            )
                        })
                        .unwrap_or(CapturedField::Omitted(super::Omission::Policy));
                    EvidenceKind::StreamText {
                        attempt_id: self.id,
                        index,
                        capture,
                    }
                });
            }
        }
    }

    pub(crate) fn stream_closed(&self, chunks: u64, text_chunks: u64, in_band_error: bool) {
        self.record(EvidenceKind::StreamObserved {
            attempt_id: self.id,
            chunks,
            text_chunks,
            in_band_error,
            prefix_limited: text_chunks > 8,
        });
    }

    pub(crate) async fn adapter_scope<T>(
        attempt: Option<&Self>,
        future: impl Future<Output = T>,
    ) -> T {
        let transport = attempt.cloned().map(|attempt| (attempt, Cell::new(false)));
        TRANSPORT.scope(transport, future).await
    }
}

tokio::task_local! {
    static TRANSPORT: Option<(AttemptObservation, Cell<bool>)>;
}

/// Host adapter opt-in. Call after actually observing transport send from
/// inside the polled adapter future. Uninstrumented adapters remain unknown.
/// This signal does not prove receipt or execution by the provider.
pub fn mark_request_sent() {
    let _ = TRANSPORT.try_with(|scope| {
        if let Some((attempt, reported)) = scope {
            if !reported.replace(true) {
                attempt.record(EvidenceKind::TransportSent {
                    attempt_id: attempt.id,
                });
            }
        }
    });
}

fn identity(value: &str) -> Option<String> {
    (value.len() <= 128
        && !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:".contains(&b))
        && !value.contains("://")
        && !super::content::restricted_text(value))
    .then(|| value.to_owned())
}
