//! Optional, host-owned local evidence. It is independent of Session replay.
//!
//! Use a dedicated database per SessionStore and reattach capture explicitly on
//! resume. The host controls access to the database and the SessionStore.

mod binding;
mod content;
mod lineage;
mod metadata;
mod outcome;
mod records;
mod store;

pub use binding::EvidenceSession;
pub(crate) use binding::RunEvidence;
pub use content::{
    reconstruct_input, AdapterInput, CapturedField, EvidenceContentPolicy, EvidenceField, Omission,
};
pub(crate) use lineage::CompactionObservation;
pub use lineage::{projection_origin, CompactionOutcome, MessageOrigin, ProjectionOrigin};
pub use metadata::{AttemptMetadata, AttemptPage, AttemptReader};
pub(crate) use outcome::AttemptObservation;
pub use outcome::{
    mark_request_sent, AdapterFailure, AttemptPurpose, EvidenceEventKind, EvidenceEventRef,
    HookDisposition,
};
pub use records::{
    AttemptOutcome, EvidenceError, EvidenceKind, EvidenceLimits, EvidencePage, EvidenceRecord,
    LocalDecision,
};
pub use store::EvidenceStore;
#[cfg(feature = "sqlite-session")]
pub(crate) use store::EVIDENCE_APPLICATION_ID;

#[cfg(test)]
mod tests;
