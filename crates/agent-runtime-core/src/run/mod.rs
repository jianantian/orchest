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
pub(crate) mod webhook;

pub use config::{
    AgentConfig, AgentConfigBuilder, AgentRun, ApprovalMode, CompactionConfig, ConfigError,
    ModelConfig, RunId, RunState, RunStatus, RuntimeConfig, SkillsConfig, SubAgentRuntime,
};
pub use handle::{ApprovalBus, EventReceiver, RunHandle};
pub use retry::{BackoffStrategy, RetryPolicy};

use std::sync::{Arc, Mutex};

use ractor::Actor;
use tokio::sync::mpsc;

use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;

use actor::{AgentRunArgs, WorkerActor};

const EVENT_CHANNEL_CAPACITY: usize = 256;

impl AgentRun {
    pub fn start(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        Self::start_with_bus(config, input, model, registry, ApprovalBus::default())
    }

    pub(crate) fn start_with_bus(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        approval_bus: ApprovalBus,
    ) -> (RunHandle, EventReceiver) {
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
        };

        // actor_ref is shared between the background task and RunHandle.
        // It is set once Actor::spawn() completes (after pre_start returns).
        let actor_ref_shared: Arc<Mutex<Option<ractor::ActorRef<actor::AgentMsg>>>> =
            Arc::new(Mutex::new(None));
        let actor_ref_for_task = actor_ref_shared.clone();

        let actor_join = tokio::spawn(async move {
            let (aref, actor_handle) = Actor::spawn(None, WorkerActor, args)
                .await
                .expect("WorkerActor spawn failed");
            if let Ok(mut guard) = actor_ref_for_task.lock() {
                *guard = Some(aref);
            }
            let _ = actor_handle.await;
        });

        let handle = RunHandle {
            run_id,
            actor_ref: actor_ref_shared,
            actor_join,
            approval_bus,
        };
        (handle, event_rx)
    }
}

#[cfg(test)]
mod tests;
