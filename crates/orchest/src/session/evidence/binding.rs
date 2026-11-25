use std::sync::Arc;

use uuid::Uuid;

use super::{
    EvidenceContentPolicy, EvidenceError, EvidenceKind, EvidencePage, EvidenceRecord, EvidenceStore,
};
use crate::run::RunId;
use crate::session::{SessionEvent, SessionEventKind, SessionStore};

#[derive(Clone)]
pub struct EvidenceSession {
    pub(super) database: EvidenceStore,
    pub(super) store: Arc<dyn SessionStore>,
    pub(super) session_id: String,
    pub(super) content: Option<EvidenceContentPolicy>,
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

    pub(super) async fn instance(&self) -> Result<RunId, EvidenceError> {
        let first = self
            .store
            .read_event(&self.session_id, 1)
            .await
            .map_err(|_| EvidenceError::SessionUnavailable)?;
        session_instance(first.as_ref()).ok_or(EvidenceError::SessionUnavailable)
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
    pub(super) database: EvidenceStore,
    pub(super) instance: RunId,
    worker_id: Uuid,
    pub(super) content: Option<EvidenceContentPolicy>,
}

impl RunEvidence {
    pub(crate) fn record(&self, run_id: RunId, step: u32, kind: EvidenceKind) {
        self.database
            .record_with(self.instance, || self.make_record(run_id, step, kind));
    }

    pub(super) fn record_with(
        &self,
        run_id: RunId,
        step: u32,
        kind: impl FnOnce() -> EvidenceKind,
    ) {
        self.database
            .record_with(self.instance, || self.make_record(run_id, step, kind()));
    }

    pub(super) fn make_record(
        &self,
        run_id: RunId,
        step: u32,
        kind: EvidenceKind,
    ) -> EvidenceRecord {
        EvidenceRecord {
            session_instance: self.instance,
            worker_id: self.worker_id,
            run_id,
            step,
            at_ms: crate::session::log::now_ms(),
            kind,
        }
    }
}
