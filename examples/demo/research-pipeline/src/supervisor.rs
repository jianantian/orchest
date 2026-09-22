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

/// `AgentRun::start_with_watchers` pre-wires declared watchers before the first
/// runtime event (`RunStarted`) and before the first model call.
pub const LIVE_ATTACHMENT_BOUNDARY: &str =
    "start_with_watchers: first-event observation guaranteed from RunStarted";

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

/// Result of resolving the public delegated-child control surface from the
/// supervisor [`RunHandle`] after observing `SubAgentStarted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedDelegatedWorkerTarget {
    pub owned_supervisor_run_id: RunId,
    pub observed_child_run_id: RunId,
    pub child_parent_run_id: RunId,
}

/// Resolve the public child control surface for a delegated worker without
/// consuming the supervisor event channel or waiting the supervisor handle.
pub async fn resolve_delegated_worker_target(
    supervisor_handle: &RunHandle,
    observed_child_run_id: RunId,
) -> Option<ResolvedDelegatedWorkerTarget> {
    let child = supervisor_handle.child(observed_child_run_id).await?;
    Some(ResolvedDelegatedWorkerTarget {
        owned_supervisor_run_id: supervisor_handle.run_id,
        observed_child_run_id: child.run_id,
        child_parent_run_id: child.parent_run_id,
    })
}

/// Look up the child handle and wait for its terminal outcome without draining
/// the supervisor [`orchest::run::EventReceiver`].
pub async fn await_delegated_worker_completion(
    supervisor_handle: &RunHandle,
    observed_child_run_id: RunId,
) -> Result<orchest::run::ChildRunOutcome, String> {
    let child = supervisor_handle
        .child(observed_child_run_id)
        .await
        .ok_or_else(|| format!("no public child control surface for {observed_child_run_id}"))?;
    child.wait_completion().await.map_err(|err| err.to_string())
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
    let config = AgentConfig::builder("research-supervisor", "research-pipeline/supervisor")
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

/// Starts the live-shaped supervisor with both watchers pre-wired via
/// [`AgentRun::start_with_watchers`], so observation begins at `RunStarted`.
pub async fn start_with_live_watchers(
    config: AgentConfig,
    input: String,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
) -> Result<StartedSupervisor, ConfigError> {
    let watcher_model = Arc::clone(&model);

    let watcher_events = Arc::new(Mutex::new(Vec::new()));
    let watcher_terminal_processed = Arc::new(Notify::new());
    let action_watcher = Arc::new(RecordingActionWatcher::observing_until_terminal(
        Arc::clone(&watcher_events),
        Arc::clone(&watcher_terminal_processed),
    ));

    let llm_watcher = LlmWatcher::builder()
        .eval_interval(1)
        .model(Arc::clone(&watcher_model))
        .build()?;
    let llm_watcher_completed_events = Arc::new(Mutex::new(Vec::new()));
    let llm_watcher_terminal_processed = Arc::new(Notify::new());
    let llm_wrapper = Arc::new(RecordingLlmWatcher::until_terminal(
        llm_watcher,
        Arc::clone(&llm_watcher_completed_events),
        None,
        Arc::clone(&llm_watcher_terminal_processed),
    ));

    let (handle, events) = AgentRun::start_with_watchers(
        config,
        input.into(),
        model,
        registry,
        vec![(action_watcher, 1024), (llm_wrapper, 1024)],
    )?;

    Ok(StartedSupervisor {
        handle,
        events,
        watcher_events,
        watcher_terminal_processed,
        llm_watcher_completed_events,
        llm_watcher_terminal_processed,
    })
}
