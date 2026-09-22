//! Deterministic multi-watcher action arbitration (SB-7 / #254).
//!
//! Watchers still receive events concurrently on independent tasks, but actions
//! for the same fan-out wave are collected before any mutation is applied to
//! the run. Slow `on_event` completion cannot reorder the effective outcome.
//!
//! # Contract
//!
//! For each successfully delivered fan-out of a runtime event to a cohort of
//! watchers:
//!
//! 1. **Gate** — wait until every watcher in that delivery cohort returns from
//!    `on_event` before applying any action from the wave.
//! 2. **Precedence** — `Abort` > `Steer` > `Inject` > `Continue`.
//! 3. **Abort** — if any watcher returns `Abort`, apply exactly one: the
//!    `Abort` from the lowest registration index; discard all other actions.
//! 4. **Otherwise** — apply every `Inject` / `Steer` in ascending registration
//!    index order (`Continue` is a no-op).
//!
//! Registration index is the order watchers were attached
//! ([`crate::run::AgentRun::start_with_watchers`] order, then subsequent
//! [`crate::run::RunHandle::attach_watcher`] calls).
//!
//! Per-watcher delivery FIFO and secondary [`crate::events::RuntimeEvent::EventsDropped`]
//! loss recovery are separate contracts; this module only orders *actions*.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use ractor::ActorRef;
use tokio::sync::Notify;

use crate::events::FanoutWaveBus;

use super::actor::{AgentMsg, CancelCmd, InjectCmd, SteerCmd};
use super::watcher::WatcherAction;

/// Ticket correlating a received channel message with its fan-out wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryTicket {
    /// Participate in arbitration for this fan-out sequence.
    Wave(u64),
    /// Per-subscriber signal (e.g. `EventsDropped`); apply solo.
    Signal,
}

/// Result of resolving one wave of watcher actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArbitratedActions {
    /// Run should abort with this reason.
    Abort(String),
    /// Non-abort control actions in registration order (Inject / Steer only).
    Effects(Vec<WatcherAction>),
}

/// Resolve one wave of `(registration_index, action)` pairs.
///
/// Public so callers and tests can lock the contract without spawning a run.
pub fn arbitrate_watcher_actions(actions: &[(usize, WatcherAction)]) -> ArbitratedActions {
    let mut indexed: Vec<(usize, WatcherAction)> = actions.to_vec();
    indexed.sort_by_key(|(idx, _)| *idx);

    let mut earliest_abort: Option<(usize, String)> = None;
    for (idx, action) in &indexed {
        if let WatcherAction::Abort(reason) = action {
            match &earliest_abort {
                Some((best_idx, _)) if *best_idx <= *idx => {}
                _ => earliest_abort = Some((*idx, reason.clone())),
            }
        }
    }
    if let Some((_, reason)) = earliest_abort {
        return ArbitratedActions::Abort(reason);
    }

    let effects = indexed
        .into_iter()
        .filter_map(|(_, action)| match action {
            WatcherAction::Inject(_) | WatcherAction::Steer(_) => Some(action),
            WatcherAction::Continue | WatcherAction::Abort(_) => None,
        })
        .collect();
    ArbitratedActions::Effects(effects)
}

#[derive(Default)]
struct WaveState {
    members: HashSet<usize>,
    sealed: bool,
    actions: HashMap<usize, WatcherAction>,
    /// Watcher index that claimed apply responsibility for this wave.
    applier: Option<usize>,
}

struct ArbitratorInner {
    active: HashSet<usize>,
    tickets: HashMap<usize, VecDeque<DeliveryTicket>>,
    waves: HashMap<u64, WaveState>,
    completed: HashSet<u64>,
}

/// Coordinates delivery tickets and gated action application for one run.
pub(crate) struct ActionArbitrator {
    inner: Mutex<ArbitratorInner>,
    notify: Notify,
    actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    /// Test/observation log of resolved waves (SB-7 adversarial proofs).
    applied_log: Mutex<Vec<ArbitratedActions>>,
}

impl ActionArbitrator {
    pub(crate) fn new(actor_ref: Arc<Mutex<Option<ActorRef<AgentMsg>>>>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(ArbitratorInner {
                active: HashSet::new(),
                tickets: HashMap::new(),
                waves: HashMap::new(),
                completed: HashSet::new(),
            }),
            notify: Notify::new(),
            actor_ref,
            applied_log: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn register_watcher(&self, index: usize) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.active.insert(index);
        inner.tickets.entry(index).or_default();
    }

    pub(crate) fn take_ticket(&self, watcher_index: usize) -> DeliveryTicket {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .tickets
            .get_mut(&watcher_index)
            .and_then(|q| q.pop_front())
            .unwrap_or(DeliveryTicket::Signal)
    }

    pub(crate) async fn submit_wave(&self, seq: u64, watcher_index: usize, action: WatcherAction) {
        loop {
            enum Next {
                Apply(ArbitratedActions),
                Done,
                Wait,
            }
            let next = {
                let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
                if inner.completed.contains(&seq) {
                    Next::Done
                } else {
                    let wave = inner.waves.entry(seq).or_default();
                    wave.actions.insert(watcher_index, action.clone());
                    let ready = wave.sealed
                        && !wave.members.is_empty()
                        && wave.members.iter().all(|m| wave.actions.contains_key(m));
                    if ready {
                        if wave.applier.is_none() {
                            wave.applier = Some(watcher_index);
                            let mut pairs: Vec<(usize, WatcherAction)> = wave
                                .members
                                .iter()
                                .filter_map(|m| wave.actions.get(m).map(|a| (*m, a.clone())))
                                .collect();
                            pairs.sort_by_key(|(i, _)| *i);
                            let resolved = arbitrate_watcher_actions(&pairs);
                            inner.waves.remove(&seq);
                            inner.completed.insert(seq);
                            if inner.completed.len() > 4096 {
                                let stale = inner.completed.iter().copied().min();
                                if let Some(s) = stale {
                                    inner.completed.remove(&s);
                                }
                            }
                            Next::Apply(resolved)
                        } else {
                            Next::Wait
                        }
                    } else {
                        Next::Wait
                    }
                }
            };
            match next {
                Next::Apply(resolved) => {
                    self.apply_resolved(resolved).await;
                    self.notify.notify_waiters();
                    return;
                }
                Next::Done => return,
                Next::Wait => {
                    let notified = self.notify.notified();
                    // Double-check after installing waiter to avoid lost wakeups.
                    {
                        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
                        if inner.completed.contains(&seq) {
                            return;
                        }
                        if let Some(wave) = inner.waves.get(&seq) {
                            let ready = wave.sealed
                                && !wave.members.is_empty()
                                && wave.members.iter().all(|m| wave.actions.contains_key(m))
                                && wave.applier.is_none();
                            if ready {
                                // Claimable now; skip wait and re-enter loop.
                                continue;
                            }
                        }
                    }
                    notified.await;
                }
            }
        }
    }

    pub(crate) async fn submit_signal(&self, watcher_index: usize, action: WatcherAction) {
        match action {
            WatcherAction::Continue => {}
            other => {
                self.apply_resolved(arbitrate_watcher_actions(&[(watcher_index, other)]))
                    .await;
            }
        }
    }

    async fn wait_actor_ref(&self) -> Option<ActorRef<AgentMsg>> {
        // Worker publishes actor_ref after its pre_start (which emits RunStarted).
        // Watchers may finish arbitration before that write; wait briefly.
        for _ in 0..500 {
            if let Ok(guard) = self.actor_ref.lock() {
                if let Some(ref aref) = *guard {
                    return Some(aref.clone());
                }
            }
            tokio::task::yield_now().await;
        }
        None
    }

    /// Test-only drain of applied arbitration outcomes (used by SB-7 proofs).
    #[cfg(test)]
    pub(crate) fn take_applied_log(&self) -> Vec<ArbitratedActions> {
        std::mem::take(&mut *self.applied_log.lock().unwrap_or_else(|p| p.into_inner()))
    }

    async fn apply_resolved(&self, resolved: ArbitratedActions) {
        if let Ok(mut log) = self.applied_log.lock() {
            log.push(resolved.clone());
        }
        let Some(aref) = self.wait_actor_ref().await else {
            return;
        };
        match resolved {
            ArbitratedActions::Abort(reason) => {
                let _ = aref.cast(AgentMsg::Cancel(CancelCmd {
                    reason: Some(reason),
                }));
            }
            ArbitratedActions::Effects(effects) => {
                for action in effects {
                    match action {
                        WatcherAction::Inject(message) => {
                            let _ = aref.cast(AgentMsg::Inject(InjectCmd { message }));
                        }
                        WatcherAction::Steer(instruction) => {
                            let _ = aref.cast(AgentMsg::Steer(SteerCmd { instruction }));
                        }
                        WatcherAction::Continue | WatcherAction::Abort(_) => {}
                    }
                }
            }
        }
    }
}

impl FanoutWaveBus for ActionArbitrator {
    fn push_wave_ticket(&self, watcher_index: usize, fanout_seq: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if !inner.active.contains(&watcher_index) {
            return;
        }
        inner
            .tickets
            .entry(watcher_index)
            .or_default()
            .push_back(DeliveryTicket::Wave(fanout_seq));
        inner
            .waves
            .entry(fanout_seq)
            .or_default()
            .members
            .insert(watcher_index);
    }

    fn push_signal_ticket(&self, watcher_index: usize) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if !inner.active.contains(&watcher_index) {
            return;
        }
        inner
            .tickets
            .entry(watcher_index)
            .or_default()
            .push_back(DeliveryTicket::Signal);
    }

    fn rollback_last_ticket(&self, watcher_index: usize) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(ticket) = inner
            .tickets
            .get_mut(&watcher_index)
            .and_then(|q| q.pop_back())
        else {
            return;
        };
        if let DeliveryTicket::Wave(seq) = ticket {
            if let Some(wave) = inner.waves.get_mut(&seq) {
                wave.members.remove(&watcher_index);
                wave.actions.remove(&watcher_index);
            }
        }
    }

    fn seal_wave(&self, fanout_seq: u64) {
        let should_notify = {
            let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            if inner.completed.contains(&fanout_seq) {
                false
            } else {
                let wave = inner.waves.entry(fanout_seq).or_default();
                wave.sealed = true;
                if wave.members.is_empty() {
                    inner.waves.remove(&fanout_seq);
                    inner.completed.insert(fanout_seq);
                    false
                } else {
                    true
                }
            }
        };
        if should_notify {
            self.notify.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_outranks_steer_inject_continue() {
        let resolved = arbitrate_watcher_actions(&[
            (2, WatcherAction::Inject("i".into())),
            (0, WatcherAction::Steer("s".into())),
            (1, WatcherAction::Abort("stop".into())),
            (3, WatcherAction::Continue),
        ]);
        assert_eq!(resolved, ArbitratedActions::Abort("stop".into()));
    }

    #[test]
    fn earliest_abort_wins_among_aborts() {
        let resolved = arbitrate_watcher_actions(&[
            (2, WatcherAction::Abort("late".into())),
            (0, WatcherAction::Abort("early".into())),
            (1, WatcherAction::Inject("i".into())),
        ]);
        assert_eq!(resolved, ArbitratedActions::Abort("early".into()));
    }

    #[test]
    fn inject_and_steer_apply_in_registration_order() {
        let resolved = arbitrate_watcher_actions(&[
            (2, WatcherAction::Inject("i2".into())),
            (0, WatcherAction::Steer("s0".into())),
            (1, WatcherAction::Continue),
            (3, WatcherAction::Steer("s3".into())),
        ]);
        assert_eq!(
            resolved,
            ArbitratedActions::Effects(vec![
                WatcherAction::Steer("s0".into()),
                WatcherAction::Inject("i2".into()),
                WatcherAction::Steer("s3".into()),
            ])
        );
    }

    #[test]
    fn all_continue_yields_empty_effects() {
        let resolved = arbitrate_watcher_actions(&[
            (0, WatcherAction::Continue),
            (1, WatcherAction::Continue),
        ]);
        assert_eq!(resolved, ArbitratedActions::Effects(vec![]));
    }
}
