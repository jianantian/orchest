// RunHandle, ApprovalSlot, EventReceiver, and approval routing logic.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, Mutex};

use crate::events::RuntimeEvent;

use super::config::RunId;

pub type EventReceiver = mpsc::Receiver<RuntimeEvent>;

/// Shared slot for a pending approval oneshot sender.  When a tool
/// requires approval, the run loop creates a oneshot pair, stores the
/// sender here, emits `ApprovalRequested`, and awaits the receiver.
/// `respond_approval` takes the sender out of the slot and sends the
/// verdict.  This guarantees that an approval response can only be
/// consumed by the request it was intended for.
pub(crate) type ApprovalSlot = Arc<Mutex<Option<oneshot::Sender<bool>>>>;

pub struct RunHandle {
    pub run_id: RunId,
    pub(crate) task: tokio::task::JoinHandle<()>,
    pub(crate) pending_approval: ApprovalSlot,
    pub(crate) active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
}

impl RunHandle {
    pub async fn wait(self) {
        let _ = self.task.await;
    }

    /// Route an approval response to the matching active run.
    ///
    /// If `run_id` matches this handle's own `run_id`, the approval goes
    /// to the root run.  If it matches a currently-active child run, the
    /// approval is forwarded there.  Returns `Err` if no approval is
    /// pending for the given `run_id` or the run is unknown.
    pub async fn respond_approval(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        let slot = if run_id == self.run_id {
            &self.pending_approval
        } else {
            let children = self.active_children.lock().await;
            let child_slot = children.get(&run_id).cloned();
            return match child_slot {
                Some(slot) => take_and_send(&slot, approved, run_id).await,
                None => Err(format!(
                    "unknown run_id {run_id}: no active run or child with that id"
                )),
            };
        };
        take_and_send(slot, approved, run_id).await
    }
}

pub(crate) async fn take_and_send(
    slot: &ApprovalSlot,
    approved: bool,
    run_id: RunId,
) -> Result<(), String> {
    let sender = slot.lock().await.take();
    match sender {
        Some(tx) => tx
            .send(approved)
            .map_err(|_| format!("run {run_id} is no longer waiting for approval")),
        None => Err(format!("no approval pending for run {run_id}")),
    }
}
