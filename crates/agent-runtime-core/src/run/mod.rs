//! Agent run orchestration, split into single-responsibility modules.

pub(crate) mod actor;
pub(crate) mod compaction;
pub(crate) mod config;
pub(crate) mod handle;
pub(crate) mod helpers;
pub mod llm_watcher;
pub(crate) mod retry;
pub(crate) mod skills;
pub(crate) mod supervisor;
pub(crate) mod tool_exec;
pub mod watcher;
pub(crate) mod webhook;

pub use config::{
    AgentConfig, AgentConfigBuilder, AgentRun, ApprovalMode, CompactionConfig, ConfigError,
    ModelConfig, RepeatedFailureConfig, RunId, RunState, RunStatus, RuntimeConfig, SkillsConfig,
    SubAgentRuntime, SupervisionStrategy,
};
pub use handle::{ApprovalBus, EventReceiver, RunHandle};
pub use retry::{BackoffStrategy, RetryPolicy};
pub use watcher::{Watcher, WatcherAction};

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;

use actor::{AgentRunArgs, ResumeState};

pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 256;

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
        supervisor::spawn_supervised(run_id, args, approval_bus, event_rx)
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
        supervisor::spawn_supervised(run_id, args, approval_bus, event_rx)
    }
}

#[cfg(test)]
mod tests;
