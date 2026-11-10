//! RunHandle, EventReceiver, child-run registry, and request routing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ractor::ActorRef;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, oneshot, watch, Notify};
use tokio::task::JoinHandle;

use crate::events::{RunFailureKind, RuntimeEvent};

use super::actor::{AgentMsg, InjectCmd, SteerCmd};
use super::cancel::{CancelToken, Cancellation};
use super::config::RunId;
use super::request::{PendingRequest, RequestBus, RequestError, RequestId, RequestResponse};
use super::supervisor::SupervisorMsg;
use super::watcher::Watcher;

pub type EventReceiver = mpsc::Receiver<RuntimeEvent>;

/// Terminal outcome of a delegated child started by [`crate::tool::agent_as_tool::AgentAsTool`].
///
/// Published on the child control surface when the child run completes or fails,
/// independently of the supervisor [`EventReceiver`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChildRunOutcome {
    Completed { output: Value },
    Failed { error: String, kind: RunFailureKind },
}

/// Error awaiting [`ChildRunHandle::wait_completion`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChildCompletionError {
    /// The registry entry was dropped before a terminal outcome was published.
    Disconnected,
}

impl std::fmt::Display for ChildCompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "child completion channel disconnected"),
        }
    }
}

impl std::error::Error for ChildCompletionError {}

/// Sets `token` and, the first time, wakes the worker so a run between steps
/// notices at once.
fn cancel_run(
    token: &CancelToken,
    actor_ref: &Mutex<Option<ActorRef<AgentMsg>>>,
    cancellation: Cancellation,
) -> bool {
    if !token.cancel(cancellation) {
        return false;
    }
    if let Ok(guard) = actor_ref.lock() {
        if let Some(ref aref) = *guard {
            let _ = aref.cast(AgentMsg::Cancel);
        }
    }
    true
}

struct RegisteredChild {
    parent_run_id: RunId,
    cancel: CancelToken,
    actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    ready: Arc<Notify>,
    supervisor_ref: Arc<Mutex<Option<ActorRef<SupervisorMsg>>>>,
    outcome_tx: watch::Sender<Option<ChildRunOutcome>>,
    /// Retained so `outcome_tx.send` still updates the cell before any
    /// external [`ChildRunHandle`] has subscribed.
    _outcome_rx: watch::Receiver<Option<ChildRunOutcome>>,
}

/// Tree-scoped registry of active delegated children.
///
/// Shared like [`RequestBus`]: the supervisor run and every nested
/// `AgentAsTool` child share one instance so application code holding the
/// supervisor [`RunHandle`] can resolve child control surfaces by run id.
#[derive(Clone, Default)]
pub struct ChildRunRegistry {
    inner: Arc<AsyncMutex<HashMap<RunId, RegisteredChild>>>,
}

impl std::fmt::Debug for ChildRunRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildRunRegistry").finish_non_exhaustive()
    }
}

/// Inputs for [`ChildRunRegistry::register`].
pub(crate) struct ChildRegistration {
    pub run_id: RunId,
    pub parent_run_id: RunId,
    pub cancel: CancelToken,
    pub actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    pub ready: Arc<Notify>,
    pub supervisor_ref: Arc<Mutex<Option<ActorRef<SupervisorMsg>>>>,
}

impl ChildRunRegistry {
    /// Register a newly started delegated child. Returns the watch sender the
    /// starter uses to publish the terminal outcome.
    pub(crate) async fn register(
        &self,
        reg: ChildRegistration,
    ) -> watch::Sender<Option<ChildRunOutcome>> {
        let (outcome_tx, outcome_rx) = watch::channel(None);
        let entry = RegisteredChild {
            parent_run_id: reg.parent_run_id,
            cancel: reg.cancel,
            actor_ref: reg.actor_ref,
            ready: reg.ready,
            supervisor_ref: reg.supervisor_ref,
            outcome_tx: outcome_tx.clone(),
            _outcome_rx: outcome_rx,
        };
        self.inner.lock().await.insert(reg.run_id, entry);
        outcome_tx
    }

    /// Resolve a public control surface for a registered child.
    pub async fn get(&self, child_run_id: RunId) -> Option<ChildRunHandle> {
        let guard = self.inner.lock().await;
        let entry = guard.get(&child_run_id)?;
        Some(ChildRunHandle {
            run_id: child_run_id,
            parent_run_id: entry.parent_run_id,
            cancel: entry.cancel.clone(),
            actor_ref: Arc::clone(&entry.actor_ref),
            ready: Arc::clone(&entry.ready),
            supervisor_ref: Arc::clone(&entry.supervisor_ref),
            outcome_rx: entry.outcome_tx.subscribe(),
        })
    }

    /// All currently registered delegated children (active or terminal-but-not-yet-unregistered).
    pub async fn list(&self) -> Vec<ChildRunHandle> {
        let guard = self.inner.lock().await;
        guard
            .iter()
            .map(|(run_id, entry)| ChildRunHandle {
                run_id: *run_id,
                parent_run_id: entry.parent_run_id,
                cancel: entry.cancel.clone(),
                actor_ref: Arc::clone(&entry.actor_ref),
                ready: Arc::clone(&entry.ready),
                supervisor_ref: Arc::clone(&entry.supervisor_ref),
                outcome_rx: entry.outcome_tx.subscribe(),
            })
            .collect()
    }
}

/// Public control surface for a delegated child started by `AgentAsTool`.
///
/// Obtained via [`RunHandle::child`] / [`RunHandle::active_children`] after
/// observing `RuntimeEvent::SubAgentStarted`. Inject and steer target this
/// child's conversation; [`Self::wait_completion`] awaits the child's terminal
/// outcome without consuming the supervisor event channel.
#[non_exhaustive]
pub struct ChildRunHandle {
    pub run_id: RunId,
    pub parent_run_id: RunId,
    cancel: CancelToken,
    actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    ready: Arc<Notify>,
    supervisor_ref: Arc<Mutex<Option<ActorRef<SupervisorMsg>>>>,
    outcome_rx: watch::Receiver<Option<ChildRunOutcome>>,
}

impl ChildRunHandle {
    /// Inject a user-role message into the **child** conversation.
    pub fn inject_message(&self, msg: &str) {
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Inject(InjectCmd {
                    message: msg.to_string(),
                }));
            }
        }
    }

    /// Inject a system-role steering instruction into the **child** conversation.
    pub fn steer(&self, instruction: &str) {
        if let Ok(guard) = self.actor_ref.lock() {
            if let Some(ref aref) = *guard {
                let _ = aref.cast(AgentMsg::Steer(SteerCmd {
                    instruction: instruction.to_string(),
                }));
            }
        }
    }

    /// Cancels the delegated child run as a user request. The parent's tool
    /// call for the child then ends with the child's abort.
    pub fn abort(&self) {
        self.cancel(Cancellation::user());
    }

    /// Cancels the delegated child run. Returns `false` when it was already
    /// cancelled; repeated calls have no further effect.
    pub fn cancel(&self, cancellation: Cancellation) -> bool {
        cancel_run(&self.cancel, &self.actor_ref, cancellation)
    }

    /// Subscribe to events emitted by the child after this call.
    ///
    /// Same lossy secondary-subscription contract as [`RunHandle::subscribe_events`].
    pub async fn subscribe_events(&self, capacity: usize) -> EventReceiver {
        let (tx, rx) = mpsc::channel(capacity);
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

    /// Attach a watcher to the delegated child run.
    ///
    /// Prefer obtaining this handle promptly after `SubAgentStarted`; attachment
    /// after the child has already emitted events is best-effort (same contract
    /// as [`RunHandle::attach_watcher`]).
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

    /// Await the child's terminal completion or failure.
    ///
    /// Does not consume the supervisor [`EventReceiver`]. Safe to call from a
    /// task that continues draining supervisor events concurrently.
    pub async fn wait_completion(&self) -> Result<ChildRunOutcome, ChildCompletionError> {
        let mut rx = self.outcome_rx.clone();
        loop {
            if let Some(outcome) = rx.borrow().clone() {
                return Ok(outcome);
            }
            rx.changed()
                .await
                .map_err(|_| ChildCompletionError::Disconnected)?;
        }
    }

    /// Snapshot the published terminal outcome, if any, without waiting.
    pub fn completion_now(&self) -> Option<ChildRunOutcome> {
        self.outcome_rx.borrow().clone()
    }
}

/// Handle to a running agent: wait for completion, abort, subscribe to events,
/// attach watchers, answer blocking requests, and steer the run mid-flight.
#[non_exhaustive]
pub struct RunHandle {
    pub run_id: RunId,
    pub(crate) cancel: CancelToken,
    pub(crate) capabilities: crate::capabilities::Capabilities,
    pub(crate) delivery_stats: super::delivery::SharedDeliveryStats,
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
    pub(crate) request_bus: RequestBus,
    /// Shared delegated-child registry for this run tree.
    pub(crate) child_registry: ChildRunRegistry,
    pub(crate) supervisor_ref: Arc<Mutex<Option<ActorRef<SupervisorMsg>>>>,
}

impl RunHandle {
    pub async fn wait(self) {
        let _ = self.actor_join.await;
    }

    /// Cancels the run as a user request. See [`Self::cancel`].
    pub fn abort(&self) {
        self.cancel(Cancellation::user());
    }

    /// Cancels the run, its delegated children and its pending requests.
    ///
    /// Takes effect at once: an in-flight model call is dropped, pending
    /// approvals resolve as cancelled, calls of interruptible tools are
    /// dropped, and no further tool call is dispatched. A tool that is not
    /// interruptible finishes and its result is recorded. The run then ends
    /// with `RunAborted` carrying `cancellation.reason`, unless it had already
    /// finished. Returns `false` when the run was already cancelled; repeated
    /// calls have no further effect.
    pub fn cancel(&self, cancellation: Cancellation) -> bool {
        cancel_run(&self.cancel, &self.actor_ref, cancellation)
    }

    /// Why the run was cancelled, once it is.
    pub fn cancellation(&self) -> Option<Cancellation> {
        self.cancel.cancellation()
    }

    /// What this run supports, as of its start.
    pub fn capabilities(&self) -> &crate::capabilities::Capabilities {
        &self.capabilities
    }

    /// How event delivery to this run's primary receiver has gone so far,
    /// including how often and how long the run waited on a full channel.
    pub fn event_delivery_stats(&self) -> super::delivery::EventDeliveryStats {
        self.delivery_stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
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

    /// Requests from this run tree (this run and its delegated children)
    /// waiting for an answer, ordered by id.
    pub async fn pending_requests(&self) -> Vec<PendingRequest> {
        self.request_bus.pending().await
    }

    /// Looks up a pending request by id.
    pub async fn request(&self, id: &RequestId) -> Option<PendingRequest> {
        self.request_bus.get(id).await
    }

    /// Answers a pending request of any run in this run tree.
    pub async fn respond(
        &self,
        id: &RequestId,
        response: RequestResponse,
    ) -> Result<(), RequestError> {
        self.request_bus.respond(id, response).await
    }

    /// Withdraws a pending request; the run treats it as not approved and
    /// continues.
    pub async fn withdraw(&self, id: &RequestId) -> Result<(), RequestError> {
        self.request_bus.withdraw(id).await
    }

    /// Resolve a public control surface for a delegated child started under
    /// this run tree by [`crate::tool::agent_as_tool::AgentAsTool`].
    ///
    /// Returns `None` if the child is not (or no longer) registered. Obtain the
    /// handle after observing [`RuntimeEvent::SubAgentStarted`] and before the
    /// child tool call returns.
    pub async fn child(&self, child_run_id: RunId) -> Option<ChildRunHandle> {
        self.child_registry.get(child_run_id).await
    }

    /// List delegated children currently registered in this run tree.
    pub async fn active_children(&self) -> Vec<ChildRunHandle> {
        self.child_registry.list().await
    }
}
