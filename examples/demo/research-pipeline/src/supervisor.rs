//! Public-API-only construction of the Research Pipeline supervisor.

use std::sync::{Arc, Mutex};

use orchest::{
    events::RuntimeEvent,
    model::ModelAdapter,
    run::{
        llm_watcher::LlmWatcher, AgentConfig, AgentRun, ConfigError, EventReceiver, RunHandle,
        RunId,
    },
    tool::{
        agent_as_tool::ContextMode,
        registry::{RegistryError, ToolRegistry},
    },
};
use thiserror::Error;
use tokio::sync::Notify;

use crate::{
    watcher::{RecordingActionWatcher, RecordingLlmWatcher},
    worker::Worker,
};

pub const RESEARCH_WORKER_TOOL: &str = "research_worker";

/// `AgentRun::start` schedules the run before returning its public handle.
/// The live path therefore attaches immediately after start as best-effort;
/// attachment before delegation or the first event is not guaranteed.
pub const LIVE_ATTACHMENT_BOUNDARY: &str =
    "best-effort immediately after AgentRun::start; first-event observation is not guaranteed";

pub struct StartedSupervisor {
    pub handle: RunHandle,
    pub events: EventReceiver,
    pub watcher_events: Arc<Mutex<Vec<RuntimeEvent>>>,
    pub watcher_terminal_processed: Arc<Notify>,
    pub llm_watcher_completed_events: Arc<Mutex<Vec<RuntimeEvent>>>,
    pub llm_watcher_terminal_processed: Arc<Notify>,
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("invalid supervisor configuration: {0}")]
    Config(#[from] ConfigError),
    #[error("supervisor tool registration failed: {0}")]
    Registry(#[from] RegistryError),
}

/// Public-surface blocker encountered while attempting to steer a delegated
/// worker observed through the supervisor event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegatedWorkerTargetBlocker {
    NoPublicChildHandleConstructorOrLookup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockedDelegatedWorkerTarget {
    pub owned_supervisor_run_id: RunId,
    pub observed_child_run_id: RunId,
    pub blocker: DelegatedWorkerTargetBlocker,
}

/// Consumes the only public handle owned by the caller and waits for that
/// supervisor run. The forwarded child id remains evidence rather than a
/// steerable target because no public constructor or lookup yields its handle.
pub async fn attempt_delegated_worker_target(
    supervisor_handle: RunHandle,
    observed_child_run_id: RunId,
) -> BlockedDelegatedWorkerTarget {
    let owned_supervisor_run_id = supervisor_handle.run_id;
    supervisor_handle.wait().await;
    BlockedDelegatedWorkerTarget {
        owned_supervisor_run_id,
        observed_child_run_id,
        blocker: DelegatedWorkerTargetBlocker::NoPublicChildHandleConstructorOrLookup,
    }
}

pub fn build_supervisor(
    worker: &Worker,
    worker_model: Arc<dyn ModelAdapter>,
    context_mode: ContextMode,
    fault: bool,
) -> Result<(AgentConfig, ToolRegistry), SupervisorError> {
    let worker_tool = worker.as_tool(
        RESEARCH_WORKER_TOOL,
        "Delegate a research request to the Research Pipeline worker.",
        worker_model,
        context_mode,
    )?;
    let scenario = if fault {
        "Delegate the request and explicitly ask the worker to call search_corpus before \
         fault_trigger. If delegation fails, return an escalation summary without retrying \
         the worker."
    } else {
        "Delegate the request, then synthesize the worker result."
    };
    let config = AgentConfig::builder("research-pipeline/supervisor")
        .system_prompt(format!(
            "You are the Research Pipeline supervisor. {scenario} Use the \
             {RESEARCH_WORKER_TOOL} tool for the delegated work."
        ))
        .max_steps(4)
        .build()?;
    let mut registry = ToolRegistry::new();
    registry.register(worker_tool)?;
    Ok((config, registry))
}

/// Starts the live-shaped supervisor, then immediately attaches both watchers.
///
/// This order is intentionally the strongest order the current public API can
/// express. It remains best-effort because `AgentRun::start` schedules model
/// execution before returning the handle used for the two registrations.
pub async fn start_with_live_watchers(
    config: AgentConfig,
    input: String,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
) -> StartedSupervisor {
    let watcher_model = Arc::clone(&model);
    let (handle, events) = AgentRun::start(config, input.into(), model, registry);

    let watcher_events = Arc::new(Mutex::new(Vec::new()));
    let watcher_terminal_processed = Arc::new(Notify::new());
    handle
        .attach_watcher(
            Arc::new(RecordingActionWatcher::observing_until_terminal(
                Arc::clone(&watcher_events),
                Arc::clone(&watcher_terminal_processed),
            )),
            1024,
        )
        .await;
    let llm_watcher = LlmWatcher::builder()
        .eval_interval(1)
        .model(Arc::clone(&watcher_model))
        .build();
    let llm_watcher_completed_events = Arc::new(Mutex::new(Vec::new()));
    let llm_watcher_terminal_processed = Arc::new(Notify::new());
    handle
        .attach_watcher(
            Arc::new(RecordingLlmWatcher::until_terminal(
                llm_watcher,
                Arc::clone(&llm_watcher_completed_events),
                None,
                Arc::clone(&llm_watcher_terminal_processed),
            )),
            1024,
        )
        .await;

    StartedSupervisor {
        handle,
        events,
        watcher_events,
        watcher_terminal_processed,
        llm_watcher_completed_events,
        llm_watcher_terminal_processed,
    }
}
