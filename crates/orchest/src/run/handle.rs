//! RunHandle, ApprovalBus, EventReceiver, and approval routing logic.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ractor::ActorRef;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, oneshot, Notify};
use tokio::task::JoinHandle;

use crate::events::RuntimeEvent;

use super::actor::{AgentMsg, CancelCmd, InjectCmd, SteerCmd};
use super::config::RunId;
use super::supervisor::SupervisorMsg;
use super::watcher::Watcher;

pub type EventReceiver = mpsc::Receiver<RuntimeEvent>;

/// Shared approval registry for an entire agent-run tree.
/// All runs (root and sub-agents at any depth) share the same instance.
#[derive(Clone, Default, Debug)]
pub struct ApprovalBus {
    pending: Arc<AsyncMutex<HashMap<RunId, oneshot::Sender<bool>>>>,
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

/// Handle to a running agent: wait for completion, abort, subscribe to events,
/// attach watchers, respond to approvals, and steer the run mid-flight.
pub struct RunHandle {
    pub run_id: RunId,
    /// Current worker actor reference.
    ///
    /// The supervisor writes this after the initial worker spawn and rewrites it
    /// after a supervised restart. The mutex is required because public handle
    /// methods and watcher reattachment tasks can read the current actor
    /// concurrently while the supervisor swaps in the restarted actor.
    pub(crate) actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    /// Fires once after `actor_ref` is set; used by `subscribe_events` to avoid
    /// a race where the subscriber cast hits `None`.
    pub(crate) ready: Arc<Notify>,
    /// Background task that owns the actor lifecycle.
    pub(crate) actor_join: JoinHandle<()>,
    pub(crate) approval_bus: ApprovalBus,
    pub(crate) supervisor_ref: Arc<Mutex<Option<ActorRef<SupervisorMsg>>>>,
}

impl RunHandle {
    pub async fn wait(self) {
        let _ = self.actor_join.await;
    }

    pub fn abort(&self) {
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Cancel(CancelCmd { reason: None }));
            }
        }
    }

    /// Inject a user-role message into the running agent's conversation.
    pub fn inject_message(&self, msg: &str) {
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Inject(InjectCmd {
                    message: msg.to_string(),
                }));
            }
        }
    }

    /// Inject a system-role steering instruction into the running agent's conversation.
    pub fn steer(&self, instruction: &str) {
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Steer(SteerCmd {
                    instruction: instruction.to_string(),
                }));
            }
        }
    }

    /// Subscribe to events emitted after this call.
    ///
    /// Secondary subscriptions are lossy under backpressure: when the channel
    /// is full, event payloads are dropped and a coalesced
    /// [`RuntimeEvent::EventsDropped`] (with `subscriber_id`, `count`, and
    /// `from_seq`..=`to_seq`) is delivered on *this* receiver once capacity
    /// frees, before the next accepted event. A mirror signal is also offered
    /// to the primary subscriber. Missed payloads are not replayed — after
    /// observing the loss signal, continue consuming or call this method again
    /// for a fresh channel that sees only future events. See [`crate::events::EventSink`].
    /// Recommend `capacity >= 1024`.
    pub async fn subscribe_events(&self, capacity: usize) -> EventReceiver {
        let (tx, rx) = mpsc::channel(capacity);
        // Wait until actor_ref is populated so the cast is never silently lost.
        loop {
            let notified = self.ready.notified();
            if self.actor_ref.lock().ok().and_then(|g| g.clone()).is_some() {
                break;
            }
            notified.await;
        }
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Subscribe(tx));
            }
        }
        rx
    }

    /// Attach a watcher that receives events and can inject messages or abort the run.
    ///
    /// **Best-effort:** this registers after the run has already been scheduled.
    /// Events emitted before registration completes (including `RunStarted` and
    /// possibly the first model call) may be missed. Prefer
    /// [`crate::run::AgentRun::start_with_watchers`] when observation must begin
    /// at the first runtime event.
    pub async fn attach_watcher(&self, watcher: Arc<dyn Watcher>, capacity: usize) {
        loop {
            let notified = self.ready.notified();
            let supervisor_ref = self
                .supervisor_ref
                .lock()
                .ok()
                .and_then(|guard| guard.clone());
            if let Some(sup_ref) = supervisor_ref {
                let (ack_tx, ack_rx) = oneshot::channel();
                let _ = sup_ref.cast(SupervisorMsg::RegisterWatcher(
                    Arc::clone(&watcher),
                    capacity,
                    ack_tx,
                ));
                let _ = ack_rx.await;
                return;
            }
            notified.await;
        }
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
