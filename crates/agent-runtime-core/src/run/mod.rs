//! Agent run orchestration, split into single-responsibility modules.

pub(crate) mod actor;
pub(crate) mod agent_ref;
pub(crate) mod compaction;
pub(crate) mod config;
pub(crate) mod handle;
pub(crate) mod helpers;
pub(crate) mod retry;
pub(crate) mod skills;
pub(crate) mod tool_exec;
pub mod watcher;
pub(crate) mod webhook;

pub use config::{
    AgentConfig, AgentConfigBuilder, AgentRun, ApprovalMode, CompactionConfig, ConfigError,
    ModelConfig, RunId, RunState, RunStatus, RuntimeConfig, SkillsConfig, SubAgentRuntime,
};
pub use handle::{ApprovalBus, EventReceiver, RunHandle};
pub use retry::{BackoffStrategy, RetryPolicy};
pub use watcher::{Watcher, WatcherAction};

use std::sync::{Arc, Mutex};

use ractor::Actor;
use tokio::sync::{mpsc, Notify};

use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;

use actor::{AgentRunArgs, ResumeState, WorkerActor};

const EVENT_CHANNEL_CAPACITY: usize = 256;

impl AgentRun {
    pub fn start(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        Self::start_with_bus(
            config,
            input,
            vec![],
            model,
            registry,
            ApprovalBus::default(),
        )
    }

    #[allow(clippy::too_many_arguments)] // justified: internal API collecting all run params
    pub(crate) fn start_with_bus(
        mut config: AgentConfig,
        input: String,
        initial_messages: Vec<crate::model::Message>,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        approval_bus: ApprovalBus,
    ) -> (RunHandle, EventReceiver) {
        config.register_persistence_hook();

        let run_id = RunId::new();
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

        let args = AgentRunArgs {
            run_id,
            config,
            input,
            model,
            registry,
            event_tx,
            approval_bus: approval_bus.clone(),
            resume: None,
            initial_messages,
        };

        spawn_actor(run_id, args, approval_bus, event_rx)
    }

    /// Resume a previous run from a persisted snapshot.
    pub fn resume(
        snapshot: crate::session::SessionSnapshot,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        let mut config = snapshot.active_config.clone();
        config.register_persistence_hook();

        let run_id = snapshot.run_id;
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let approval_bus = ApprovalBus::default();

        let resume = ResumeState {
            messages: snapshot.messages,
            step: snapshot.step,
            budget_used: snapshot.budget_used,
        };

        let args = AgentRunArgs {
            run_id,
            config,
            input: String::new(),
            model,
            registry,
            event_tx,
            approval_bus: approval_bus.clone(),
            resume: Some(resume),
            initial_messages: vec![],
        };

        spawn_actor(run_id, args, approval_bus, event_rx)
    }
}

fn spawn_actor(
    run_id: RunId,
    args: AgentRunArgs,
    approval_bus: ApprovalBus,
    event_rx: mpsc::Receiver<crate::events::RuntimeEvent>,
) -> (RunHandle, EventReceiver) {
    let actor_ref_shared: Arc<Mutex<Option<ractor::ActorRef<actor::AgentMsg>>>> =
        Arc::new(Mutex::new(None));
    let actor_ref_for_task = actor_ref_shared.clone();

    let ready = Arc::new(Notify::new());
    let ready_for_task = Arc::clone(&ready);

    let actor_join = tokio::spawn(async move {
        let (aref, actor_handle) = Actor::spawn(None, WorkerActor, args)
            .await
            .expect("WorkerActor spawn failed");
        if let Ok(mut guard) = actor_ref_for_task.lock() {
            *guard = Some(aref);
        }
        ready_for_task.notify_waiters();
        let _ = actor_handle.await;
    });
    let handle = RunHandle {
        run_id,
        actor_ref: actor_ref_shared,
        ready,
        actor_join,
        approval_bus,
    };
    (handle, event_rx)
}

#[cfg(test)]
mod tests;
