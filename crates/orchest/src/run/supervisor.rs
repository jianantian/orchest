//! SupervisorActor: monitors WorkerActor, implements crash recovery via supervision tree.

use std::sync::{Arc, Mutex};

use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tokio::sync::{mpsc, oneshot};

use crate::events::{FanoutWaveBus, RuntimeEvent};
use crate::model::ModelAdapter;
use crate::session::SessionStore;
use crate::tool::registry::ToolRegistry;

use super::action_arbitration::{ActionArbitrator, DeliveryTicket};
use super::actor::{AgentMsg, AgentRunArgs, ResumeState, WorkerActor};
use super::config::{RunId, SupervisionStrategy};
use super::handle::ApprovalBus;
use super::watcher::Watcher;

#[allow(dead_code)] // justified: Shutdown reserved for graceful supervisor teardown from RunHandle
pub(crate) enum SupervisorMsg {
    RegisterWatcher(Arc<dyn Watcher>, usize, oneshot::Sender<()>),
    Shutdown,
}

pub(crate) struct SupervisorState {
    strategy: SupervisionStrategy,
    attempts: u32,
    run_id: RunId,
    actor_ref_shared: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    ready: Arc<tokio::sync::Notify>,
    event_tx: mpsc::Sender<RuntimeEvent>,
    watchers: Vec<(Arc<dyn Watcher>, usize)>,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    config: crate::run::AgentConfig,
    approval_bus: ApprovalBus,
    session_store: Option<Arc<dyn SessionStore>>,
    session_id: Option<String>,
    original_input: Vec<crate::model::ContentBlock>,
    original_initial_messages: Vec<crate::model::Message>,
    original_resume: Option<ResumeState>,
    worker_handle: Option<ractor::concurrency::JoinHandle<()>>,
    arbitrator: std::sync::Arc<ActionArbitrator>,
}

pub(crate) struct SupervisorArgs {
    pub run_id: RunId,
    pub config: crate::run::AgentConfig,
    pub model: Arc<dyn ModelAdapter>,
    pub registry: ToolRegistry,
    pub event_tx: mpsc::Sender<RuntimeEvent>,
    pub approval_bus: ApprovalBus,
    pub actor_ref_shared: Arc<Mutex<Option<ActorRef<AgentMsg>>>>,
    pub ready: Arc<tokio::sync::Notify>,
    pub worker_args: AgentRunArgs,
    /// Watchers declared at start (`start_with_watchers`); retained for restart.
    pub initial_watchers: Vec<(Arc<dyn Watcher>, usize)>,
    pub arbitrator: std::sync::Arc<ActionArbitrator>,
}

pub(crate) struct SupervisorActor;

impl Actor for SupervisorActor {
    type Msg = SupervisorMsg;
    type State = SupervisorState;
    type Arguments = SupervisorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<SupervisorMsg>,
        args: SupervisorArgs,
    ) -> Result<SupervisorState, ActorProcessingErr> {
        let session_store = args.config.session_store.clone();
        let session_id = args.config.session_id.clone();
        let original_input = args.worker_args.input.clone();
        let original_initial_messages = args.worker_args.initial_messages.clone();
        let original_resume = args.worker_args.resume.clone();

        let (worker_ref, worker_handle) =
            Actor::spawn_linked(None, WorkerActor, args.worker_args, myself.get_cell()).await?;

        if let Ok(mut guard) = args.actor_ref_shared.lock() {
            *guard = Some(worker_ref);
        }
        args.ready.notify_waiters();

        Ok(SupervisorState {
            strategy: args.config.supervision_strategy.clone(),
            attempts: 0,
            run_id: args.run_id,
            actor_ref_shared: args.actor_ref_shared,
            ready: args.ready,
            event_tx: args.event_tx,
            watchers: args.initial_watchers,
            model: args.model,
            registry: args.registry,
            config: args.config,
            approval_bus: args.approval_bus,
            session_store,
            session_id,
            original_input,
            original_initial_messages,
            original_resume,
            worker_handle: Some(worker_handle),
            arbitrator: args.arbitrator,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<SupervisorMsg>,
        msg: SupervisorMsg,
        state: &mut SupervisorState,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            SupervisorMsg::RegisterWatcher(watcher, capacity, ack) => {
                let index = state.watchers.len();
                state.watchers.push((watcher.clone(), capacity));
                state.arbitrator.register_watcher(index);
                let worker_ref = state
                    .actor_ref_shared
                    .lock()
                    .ok()
                    .and_then(|guard| guard.clone());
                if let Some(aref) = worker_ref {
                    reattach_watcher(&aref, &watcher, capacity, index, &state.arbitrator);
                }
                let _ = ack.send(());
            }
            SupervisorMsg::Shutdown => {
                myself.stop(None);
            }
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<SupervisorMsg>,
        message: SupervisionEvent,
        state: &mut SupervisorState,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorFailed(_who, _err) => match &state.strategy {
                SupervisionStrategy::Stop => {
                    let _ = state
                        .event_tx
                        .send(RuntimeEvent::RunAborted {
                            reason: Some("worker failed (strategy: Stop)".into()),
                        })
                        .await;
                    myself.stop(None);
                }
                SupervisionStrategy::Restart { max_retries } => {
                    state.attempts += 1;
                    if state.attempts > *max_retries {
                        let _ = state
                            .event_tx
                            .send(RuntimeEvent::RunAborted {
                                reason: Some(format!("max retries ({max_retries}) exceeded")),
                            })
                            .await;
                        myself.stop(None);
                        return Ok(());
                    }

                    let _ = state
                        .event_tx
                        .send(RuntimeEvent::RunRestarted {
                            attempt: state.attempts,
                        })
                        .await;

                    let new_args = build_restart_args(state).await;

                    let (worker_ref, worker_handle) =
                        Actor::spawn_linked(None, WorkerActor, new_args, myself.get_cell()).await?;

                    if let Ok(mut guard) = state.actor_ref_shared.lock() {
                        *guard = Some(worker_ref.clone());
                    }
                    state.ready.notify_waiters();
                    state.worker_handle = Some(worker_handle);

                    for (index, (watcher, capacity)) in state.watchers.iter().enumerate() {
                        state.arbitrator.register_watcher(index);
                        reattach_watcher(&worker_ref, watcher, *capacity, index, &state.arbitrator);
                    }
                }
            },
            SupervisionEvent::ActorTerminated(_who, _state_box, _reason) => {
                myself.stop(None);
            }
            _ => {}
        }
        Ok(())
    }
}

fn reattach_watcher(
    worker_ref: &ActorRef<AgentMsg>,
    watcher: &Arc<dyn Watcher>,
    capacity: usize,
    watcher_index: usize,
    arbitrator: &Arc<ActionArbitrator>,
) {
    let (tx, rx) = mpsc::channel(capacity);
    let _ = worker_ref.cast(AgentMsg::SubscribeWatcher {
        tx,
        wave_bus: Arc::clone(arbitrator) as Arc<dyn FanoutWaveBus>,
        watcher_index,
    });
    spawn_watcher_processor(
        rx,
        Arc::clone(watcher),
        watcher_index,
        Arc::clone(arbitrator),
    );
}

/// Drive a watcher from an already-subscribed (or pre-wired) event receiver.
///
/// Actions are submitted to [`ActionArbitrator`] so multi-watcher outcomes for
/// the same fan-out wave are gated and resolved deterministically (SB-7).
fn spawn_watcher_processor(
    mut rx: mpsc::Receiver<RuntimeEvent>,
    watcher: Arc<dyn Watcher>,
    watcher_index: usize,
    arbitrator: Arc<ActionArbitrator>,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let ticket = arbitrator.take_ticket(watcher_index);
            let action = watcher.on_event(&event).await;
            match ticket {
                DeliveryTicket::Wave(seq) => {
                    arbitrator.submit_wave(seq, watcher_index, action).await;
                }
                DeliveryTicket::Signal => {
                    arbitrator.submit_signal(watcher_index, action).await;
                }
            }
        }
    });
}

/// Spawn a supervised run. `initial_watchers` are wired into the worker's
/// subscriber list before the first emit (`RunStarted`), so observation is
/// deterministic from that boundary. Capacities must already be validated
/// (`> 0`) by the public start-with-watchers entry point.
pub(crate) fn spawn_supervised(
    run_id: RunId,
    mut args: AgentRunArgs,
    approval_bus: ApprovalBus,
    event_rx: mpsc::Receiver<RuntimeEvent>,
    initial_watchers: Vec<(Arc<dyn Watcher>, usize)>,
) -> (super::handle::RunHandle, super::EventReceiver) {
    let actor_ref_shared: Arc<Mutex<Option<ActorRef<AgentMsg>>>> = Arc::new(Mutex::new(None));
    let ready = Arc::new(tokio::sync::Notify::new());

    let supervisor_ref_shared: Arc<Mutex<Option<ActorRef<SupervisorMsg>>>> =
        Arc::new(Mutex::new(None));
    let supervisor_ref_for_handle = supervisor_ref_shared.clone();
    let ready_for_supervisor_ref = ready.clone();

    let arbitrator = ActionArbitrator::new(Arc::clone(&actor_ref_shared));
    let mut initial_event_subs = Vec::with_capacity(initial_watchers.len());
    for (index, (watcher, capacity)) in initial_watchers.iter().enumerate() {
        let (tx, rx) = mpsc::channel(*capacity);
        initial_event_subs.push(tx);
        arbitrator.register_watcher(index);
        spawn_watcher_processor(rx, Arc::clone(watcher), index, Arc::clone(&arbitrator));
    }
    args.initial_event_subs = initial_event_subs;
    args.watcher_wave_bus = Some(Arc::clone(&arbitrator) as Arc<dyn FanoutWaveBus>);

    let sup_args = SupervisorArgs {
        run_id,
        config: args.config.clone(),
        model: args.model.clone(),
        registry: args.registry.clone(),
        event_tx: args.event_tx.clone(),
        approval_bus: approval_bus.clone(),
        actor_ref_shared: actor_ref_shared.clone(),
        ready: ready.clone(),
        worker_args: args,
        initial_watchers,
        arbitrator,
    };

    let actor_join = tokio::spawn(async move {
        let (sup_ref, sup_handle) = Actor::spawn(None, SupervisorActor, sup_args)
            .await
            .expect("SupervisorActor spawn failed");
        if let Ok(mut guard) = supervisor_ref_shared.lock() {
            *guard = Some(sup_ref);
        }
        ready_for_supervisor_ref.notify_waiters();
        let _ = sup_handle.await;
    });

    let handle = super::handle::RunHandle {
        run_id,
        actor_ref: actor_ref_shared,
        ready,
        actor_join,
        approval_bus,
        supervisor_ref: supervisor_ref_for_handle,
    };
    (handle, event_rx)
}

async fn build_restart_args(state: &SupervisorState) -> AgentRunArgs {
    let resume = if let (Some(store), Some(session_id)) = (&state.session_store, &state.session_id)
    {
        match store.load(session_id).await {
            Ok(Some(snapshot)) => Some(ResumeState {
                messages: snapshot.messages,
                step: snapshot.step,
                budget_used: snapshot.budget_used,
            }),
            _ => state.original_resume.clone(),
        }
    } else {
        state.original_resume.clone()
    };

    AgentRunArgs {
        run_id: state.run_id,
        config: state.config.clone(),
        input: state.original_input.clone(),
        model: state.model.clone(),
        registry: state.registry.clone(),
        event_tx: state.event_tx.clone(),
        approval_bus: state.approval_bus.clone(),
        resume,
        initial_messages: state.original_initial_messages.clone(),
        // Restart reattaches via `reattach_watcher` / Subscribe, not pre-wiring.
        initial_event_subs: vec![],
        watcher_wave_bus: None,
    }
}
