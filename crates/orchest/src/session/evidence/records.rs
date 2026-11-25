use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    AdapterFailure, AttemptPurpose, CapturedField, CompactionOutcome, EvidenceEventRef,
    EvidenceField, HookDisposition, MessageOrigin,
};
use crate::run::RunId;

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
    #[error("the evidence database must be a regular file accessible only to its owner")]
    UnsafeFilePermissions,
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
    ContextBasis {
        attempt_id: Uuid,
        through_seq: u64,
        /// If true, the input messages use the unchanged v2 projection.
        projection_order: bool,
    },
    /// Current policy starts compaction on context pressure.
    CompactionStarted {
        compaction_id: Uuid,
        through_seq: u64,
    },
    CompactionFinished {
        compaction_id: Uuid,
        outcome: CompactionOutcome,
        summary_attempt: Option<Uuid>,
    },
    CompactionApplied {
        compaction_id: Uuid,
        event: EvidenceEventRef,
        previous_through_seq: u64,
        /// Positions of the committed replacement. Absent when uncaptured.
        mapping: Option<Vec<MessageOrigin>>,
        /// Source positions passed to the summarizer when their origin is known.
        summarized: Option<Vec<usize>>,
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
    /// Stored records whose retention expired for this Session instance.
    pub expired_records: u64,
    /// This instance has been revoked and cannot accept new evidence.
    pub revoked: bool,
    /// Queue or storage drops observed by this handle since opening the store.
    pub observed_drops: u64,
}
