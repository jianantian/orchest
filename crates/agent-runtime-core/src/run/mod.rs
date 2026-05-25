// Agent run orchestration, split into single-responsibility modules.

pub(crate) mod compaction;
pub(crate) mod config;
pub(crate) mod handle;
pub(crate) mod helpers;
pub(crate) mod loop_;
pub(crate) mod skills;
pub(crate) mod sub_agent;
pub(crate) mod tool_exec;
pub(crate) mod webhook;

pub use config::{
    AgentConfig, AgentConfigBuilder, AgentRun, CompactionConfig, ModelConfig, RunId, RunState,
    RunStatus, RuntimeConfig, SkillsConfig, SubAgentRuntime,
};
pub use handle::{EventReceiver, RunHandle};

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;

impl AgentRun {
    pub fn start(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        let run_id = RunId::new();
        let (event_tx, event_rx) = mpsc::channel(256);
        let pending_approval: handle::ApprovalSlot = Arc::new(Mutex::new(None));
        let active_children: Arc<Mutex<HashMap<RunId, handle::ApprovalSlot>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let approval_for_loop = Arc::clone(&pending_approval);
        let children_for_loop = Arc::clone(&active_children);
        let task = tokio::spawn(async move {
            loop_::run_loop(
                run_id,
                config,
                input,
                model,
                registry,
                event_tx,
                approval_for_loop,
                children_for_loop,
            )
            .await;
        });

        let handle = RunHandle {
            run_id,
            task,
            pending_approval,
            active_children,
        };
        (handle, event_rx)
    }
}

#[cfg(test)]
mod tests;
