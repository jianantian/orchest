//! Agent run orchestration, split into single-responsibility modules.

pub(crate) mod compaction;
pub(crate) mod config;
pub(crate) mod handle;
pub(crate) mod helpers;
pub(crate) mod loop_;
pub(crate) mod retry;
pub(crate) mod skills;
pub(crate) mod tool_exec;
pub(crate) mod webhook;

pub use config::{
    AgentConfig, AgentConfigBuilder, AgentRun, CompactionConfig, ConfigError, ModelConfig, RunId,
    RunState, RunStatus, RuntimeConfig, SkillsConfig, SubAgentRuntime,
};
pub use handle::{ApprovalBus, EventReceiver, RunHandle};
pub use retry::{BackoffStrategy, RetryPolicy};

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;

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
        Self::start_with_bus_and_token(
            config,
            input,
            model,
            registry,
            approval_bus,
            CancellationToken::new(),
        )
    }

    #[allow(clippy::too_many_arguments)] // justified: full runtime wiring for run loop; public API uses start() with fewer args
    pub(crate) fn start_with_bus_and_token(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        approval_bus: ApprovalBus,
        cancel_token: CancellationToken,
    ) -> (RunHandle, EventReceiver) {
        let run_id = RunId::new();
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

        let bus_for_loop = approval_bus.clone();
        let token_for_loop = cancel_token.clone();
        let task = tokio::spawn(async move {
            loop_::run_loop(
                run_id,
                config,
                input,
                model,
                registry,
                event_tx,
                bus_for_loop,
                token_for_loop,
            )
            .await;
        });

        let handle = RunHandle {
            run_id,
            task,
            approval_bus,
            cancel_token,
        };
        (handle, event_rx)
    }
}

#[cfg(test)]
mod tests;
