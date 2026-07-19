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
    ModelConfig, RepeatedFailureConfig, RunId, RunInput, RunInputError, RunState, RunStatus,
    RuntimeConfig, SkillDisclosure, SkillsConfig, SubAgentRuntime, SupervisionStrategy,
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
    /// Starts a fresh run with the given input as the first user turn.
    ///
    /// `input` accepts a plain `&str`/`String` (via `Into<RunInput>`) for
    /// text-only input, or a `RunInput` built with `RunInput::text(..)`,
    /// `.with_image(..)`, or `RunInput::from_blocks(..)` to include images,
    /// video, or audio alongside text (see `RunInput` for what block kinds
    /// are valid). The model adapter and its provider determine which of
    /// those block kinds actually reach the underlying request.
    pub fn start(
        config: AgentConfig,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        Self::start_with_bus(
            config,
            input.into_blocks(),
            vec![],
            model,
            registry,
            ApprovalBus::default(),
        )
    }

    #[allow(clippy::too_many_arguments)] // justified: internal API collecting all run params
    pub(crate) fn start_with_bus(
        mut config: AgentConfig,
        input: Vec<crate::model::ContentBlock>,
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

    /// Resumes a previous run from a persisted snapshot exactly as it left
    /// off — the message history is replayed unchanged.
    ///
    /// This is for continuing an interrupted run (crash, process restart),
    /// not for asking a follow-up question: it does **not** append any new
    /// input. Calling it on a snapshot whose last turn already produced a
    /// final answer just re-invokes the model against unchanged history,
    /// producing a stale or duplicate response. For a follow-up turn, use
    /// [`AgentRun::resume_with_input`] instead.
    ///
    /// Returns `ConfigError::SessionStoreMissing` if the snapshot's
    /// `active_config.session_id` is set (persistence was enabled) but no
    /// `session_store` is attached — deserializing a snapshot always drops
    /// the store (`#[serde(skip)]`), so re-attach it first via
    /// `active_config.with_session_store(store, session_id)`.
    pub fn resume(
        snapshot: crate::session::SessionSnapshot,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> Result<(RunHandle, EventReceiver), ConfigError> {
        let mut config = snapshot.active_config.clone();
        check_session_store_attached(&config)?;
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
            input: vec![],
            model,
            registry,
            event_tx,
            approval_bus: approval_bus.clone(),
            resume: Some(resume),
            initial_messages: vec![],
        };
        Ok(supervisor::spawn_supervised(
            run_id,
            args,
            approval_bus,
            event_rx,
        ))
    }

    /// Resumes a previous run from a persisted snapshot and appends `input`
    /// as a new user turn — the follow-up-question path (e.g. "continue this
    /// session with a new question").
    ///
    /// Unlike [`AgentRun::resume`], which replays the snapshot unchanged,
    /// this pushes `input` onto the snapshot's message history before
    /// resuming, so the model is invoked against history plus the new turn.
    ///
    /// Returns `ConfigError::SessionStoreMissing` under the same condition as
    /// `resume` — see its rustdoc.
    pub fn resume_with_input(
        snapshot: crate::session::SessionSnapshot,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> Result<(RunHandle, EventReceiver), ConfigError> {
        let mut config = snapshot.active_config.clone();
        check_session_store_attached(&config)?;
        config.register_persistence_hook();
        let run_id = snapshot.run_id;
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let approval_bus = ApprovalBus::default();
        let mut messages = snapshot.messages;
        messages.push(crate::model::Message {
            role: crate::model::Role::User,
            content: input.into_blocks(),
        });
        let resume = ResumeState {
            messages,
            step: snapshot.step,
            budget_used: snapshot.budget_used,
        };
        let args = AgentRunArgs {
            run_id,
            config,
            input: vec![],
            model,
            registry,
            event_tx,
            approval_bus: approval_bus.clone(),
            resume: Some(resume),
            initial_messages: vec![],
        };
        Ok(supervisor::spawn_supervised(
            run_id,
            args,
            approval_bus,
            event_rx,
        ))
    }
}

/// A snapshot with `session_id` set had persistence enabled at snapshot time;
/// deserializing always drops `session_store` (`#[serde(skip)]`), so resuming
/// without re-attaching it would silently stop persisting. Snapshots that
/// never had persistence (`session_id: None`) are unaffected.
fn check_session_store_attached(config: &AgentConfig) -> Result<(), ConfigError> {
    match (&config.session_id, &config.session_store) {
        (Some(session_id), None) => Err(ConfigError::SessionStoreMissing {
            session_id: session_id.clone(),
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
