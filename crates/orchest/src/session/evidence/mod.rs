//! Optional, host-owned local evidence. It is independent of Session replay.
//!
//! Use a dedicated database per SessionStore and reattach capture explicitly on
//! resume. The host controls access to the database and the SessionStore.

mod content;
mod outcome;
mod store;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{SessionEvent, SessionEventKind, SessionStore};
use crate::run::RunId;

pub use content::{
    reconstruct_input, AdapterInput, CapturedField, EvidenceContentPolicy, EvidenceField, Omission,
};
pub(crate) use outcome::AttemptObservation;
pub use outcome::{
    mark_request_sent, AdapterFailure, AttemptPurpose, EvidenceEventKind, EvidenceEventRef,
    HookDisposition,
};
pub use store::EvidenceStore;

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EvidenceLimits {
    pub queue_records: usize,
    pub record_bytes: usize,
    /// Maximum size of the main SQLite database file.
    pub database_bytes: usize,
    pub retention: Duration,
}

impl Default for EvidenceLimits {
    fn default() -> Self {
        Self {
            queue_records: 64,
            record_bytes: 64 * 1024,
            database_bytes: 32 * 1024 * 1024,
            retention: Duration::from_secs(24 * 60 * 60),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EvidenceError {
    #[error("invalid evidence limits")]
    InvalidLimits,
    #[error("unsupported evidence database schema")]
    Schema,
    #[error("the evidence writer is unavailable")]
    Unavailable,
    #[error("the bound Session instance is unavailable")]
    SessionUnavailable,
    #[error("evidence storage operation failed")]
    Storage,
    #[error("evidence storage quota exceeded")]
    Full,
    #[error("evidence record failed validation")]
    InvalidRecord,
    #[error("incomplete adapter input: {field:?}")]
    IncompleteInput { field: EvidenceField },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct EvidenceRecord {
    pub session_instance: RunId,
    pub worker_id: Uuid,
    pub run_id: RunId,
    pub step: u32,
    pub at_ms: u64,
    pub kind: EvidenceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceKind {
    AttemptStarted {
        attempt_id: Uuid,
        ordinal: u32,
        purpose: AttemptPurpose,
        provider: Option<String>,
        model: Option<String>,
    },
    AttemptFinished {
        attempt_id: Uuid,
        outcome: AttemptOutcome,
    },
    Field {
        attempt_id: Uuid,
        field: EvidenceField,
        capture: CapturedField,
    },
    LocalDecision {
        decision: LocalDecision,
    },
    TransportSent {
        attempt_id: Uuid,
    },
    AdapterReturned {
        attempt_id: Uuid,
        stop_reason: Option<crate::model::StopReason>,
        input_tokens: u64,
        output_tokens: u64,
        option_adjustments: usize,
    },
    AdapterFailed {
        attempt_id: Uuid,
        failure: AdapterFailure,
    },
    AfterModel {
        attempt_id: Uuid,
        disposition: HookDisposition,
    },
    SessionEvent {
        attempt_id: Uuid,
        event: EvidenceEventRef,
    },
    StreamText {
        attempt_id: Uuid,
        index: u64,
        capture: CapturedField,
    },
    StreamObserved {
        attempt_id: Uuid,
        chunks: u64,
        text_chunks: u64,
        in_band_error: bool,
        prefix_limited: bool,
    },
    /// The original append crossed an existing SessionStore flush barrier.
    SessionDurableThrough {
        seq: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AttemptOutcome {
    Returned,
    AdapterError,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LocalDecision {
    HookBlocked,
    ContextRejected,
    CancelledBeforeAdapter,
}

/// A page never implies complete coverage of the run.
#[derive(Debug)]
#[non_exhaustive]
pub struct EvidencePage {
    pub records: Vec<EvidenceRecord>,
    pub next_cursor: Option<i64>,
    /// Invalid rows skipped in this page. Unknown loss, including crashes
    /// before enqueue or during persistence, cannot be counted.
    pub invalid_records: usize,
    /// Queue or storage drops observed by this handle since opening the store.
    pub observed_drops: u64,
}

#[derive(Clone)]
pub struct EvidenceSession {
    database: EvidenceStore,
    store: Arc<dyn SessionStore>,
    session_id: String,
    content: Option<EvidenceContentPolicy>,
}

impl EvidenceSession {
    /// Opt in to content for this run binding. Metadata capture is the default.
    pub fn with_content_policy(mut self, policy: EvidenceContentPolicy) -> Self {
        self.content = Some(policy);
        self
    }

    pub(crate) fn bind_run(
        &self,
        store: &Arc<dyn SessionStore>,
        session_id: &str,
        first: Option<&SessionEvent>,
    ) -> Option<RunEvidence> {
        if !Arc::ptr_eq(&self.store, store) || self.session_id != session_id {
            return None;
        }
        Some(RunEvidence {
            database: self.database.clone(),
            instance: session_instance(first)?,
            worker_id: Uuid::new_v4(),
            content: self.content.clone(),
        })
    }

    /// Flushes queued evidence to the local database before reading. This never
    /// flushes or otherwise changes the associated SessionStore.
    pub async fn read(
        &self,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<EvidencePage, EvidenceError> {
        self.database
            .read(self.instance().await?, cursor, limit)
            .await
    }

    /// Revoke evidence for the current instance before deleting its Session.
    /// Queued or later records for that instance cannot repopulate it.
    pub async fn revoke(&self) -> Result<(), EvidenceError> {
        self.database.revoke(self.instance().await?).await
    }

    async fn instance(&self) -> Result<RunId, EvidenceError> {
        let events = self
            .store
            .read(&self.session_id, 1)
            .await
            .map_err(|_| EvidenceError::SessionUnavailable)?;
        session_instance(events.first()).ok_or(EvidenceError::SessionUnavailable)
    }
}

fn session_instance(first: Option<&SessionEvent>) -> Option<RunId> {
    match first?.kind {
        SessionEventKind::RunStarted { run_id, .. } => Some(run_id),
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct RunEvidence {
    database: EvidenceStore,
    instance: RunId,
    worker_id: Uuid,
    content: Option<EvidenceContentPolicy>,
}

impl RunEvidence {
    pub(crate) fn record(&self, run_id: RunId, step: u32, kind: EvidenceKind) {
        self.database
            .record_with(self.instance, || self.make_record(run_id, step, kind));
    }

    fn record_with(&self, run_id: RunId, step: u32, kind: impl FnOnce() -> EvidenceKind) {
        self.database
            .record_with(self.instance, || self.make_record(run_id, step, kind()));
    }

    fn make_record(&self, run_id: RunId, step: u32, kind: EvidenceKind) -> EvidenceRecord {
        EvidenceRecord {
            session_instance: self.instance,
            worker_id: self.worker_id,
            run_id,
            step,
            at_ms: super::log::now_ms(),
            kind,
        }
    }
}

pub(crate) fn identity(value: &str) -> Option<String> {
    (value.len() <= 128
        && !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:".contains(&b))
        && !value.contains("://"))
    .then(|| value.to_owned())
}
