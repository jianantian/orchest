//! RunHandle, ApprovalBus, EventReceiver, and approval routing logic.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use crate::events::RuntimeEvent;

use super::config::RunId;

pub type EventReceiver = mpsc::Receiver<RuntimeEvent>;

/// Shared approval registry for an entire agent-run tree.
/// All runs (root and sub-agents at any depth) share the same instance.
#[derive(Clone, Default, Debug)]
pub struct ApprovalBus {
    pending: Arc<Mutex<HashMap<RunId, oneshot::Sender<bool>>>>,
}

impl ApprovalBus {
    pub async fn request(&self, run_id: RunId) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(run_id, tx);
        rx
    }

    pub async fn respond(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        match self.pending.lock().await.remove(&run_id) {
            Some(tx) => tx
                .send(approved)
                .map_err(|_| format!("run {run_id} is no longer waiting for approval")),
            None => Err(format!("no pending approval for run {run_id}")),
        }
    }

    pub async fn cancel(&self, run_id: RunId) {
        self.pending.lock().await.remove(&run_id);
    }
}

pub struct RunHandle {
    pub run_id: RunId,
    pub(crate) task: tokio::task::JoinHandle<()>,
    pub(crate) approval_bus: ApprovalBus,
    pub(crate) cancel_token: CancellationToken,
}

impl RunHandle {
    pub async fn wait(self) {
        let _ = self.task.await;
    }

    pub fn abort(&self) {
        self.cancel_token.cancel();
    }

    /// Route an approval response to any run in this run tree.
    pub async fn respond_approval(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        self.approval_bus.respond(run_id, approved).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn approval_bus_round_trip() {
        let bus = ApprovalBus::default();
        let run_id = RunId::new();
        let rx = bus.request(run_id).await;
        bus.respond(run_id, true).await.unwrap();
        assert!(rx.await.unwrap());
    }

    #[tokio::test]
    async fn approval_bus_unknown_run_id_returns_err() {
        let bus = ApprovalBus::default();
        let result = bus.respond(RunId::new(), true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn approval_bus_cancel_clears_slot() {
        let bus = ApprovalBus::default();
        let run_id = RunId::new();
        let _rx = bus.request(run_id).await;
        bus.cancel(run_id).await;
        let result = bus.respond(run_id, true).await;
        assert!(result.is_err());
    }
}
