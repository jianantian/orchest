//! Agent run orchestration, split into single-responsibility modules.

pub(crate) mod action_arbitration;
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

pub use action_arbitration::{arbitrate_watcher_actions, ArbitratedActions};
pub use config::{
    AgentConfig, AgentConfigBuilder, AgentRun, ApprovalMode, CompactionConfig, ConfigError,
    ModelConfig, RepeatedFailureConfig, RunId, RunInput, RunInputError, RunState, RunStatus,
    RuntimeConfig, SkillDisclosure, SkillsConfig, SubAgentRuntime, SupervisionStrategy,
};
pub use handle::{
    ApprovalBus, ChildCompletionError, ChildRunHandle, ChildRunOutcome, ChildRunRegistry,
    EventReceiver, RunHandle,
};
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
    ///
    /// Equivalent to [`AgentRun::start_with_messages`] with an empty
    /// `initial_messages` history.
    pub fn start(
        config: AgentConfig,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        Self::start_with_messages(config, vec![], input, model, registry)
    }

    /// Starts a run with watchers already active before the first runtime
    /// event (`RunStarted`) and before the first model call.
    ///
    /// Declared watchers are pre-wired into the worker subscriber list, so
    /// successful registration guarantees observation from that boundary
    /// without application timing assumptions. Channel `capacity` must be
    /// `> 0` for every watcher; invalid capacity returns
    /// [`ConfigError::InvalidWatcherCapacity`] **before** execution begins.
    ///
    /// For dynamic attachment after a run has already started, use
    /// [`RunHandle::attach_watcher`] — that path remains supported but is
    /// best-effort and may miss events emitted before registration completes.
    pub fn start_with_watchers(
        config: AgentConfig,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        watchers: Vec<(Arc<dyn Watcher>, usize)>,
    ) -> Result<(RunHandle, EventReceiver), ConfigError> {
        Self::start_with_messages_and_watchers(config, vec![], input, model, registry, watchers)
    }

    /// Starts a fresh run with a caller-supplied multi-turn history:
    /// `initial_messages` is the prior conversation, `input` is the new user
    /// turn. The initial message list seen by the model is assembled as
    ///
    /// ```text
    /// [System(config.system_prompt)] + initial_messages + [User(input)]
    /// ```
    ///
    /// `input` accepts the same values as [`AgentRun::start`].
    ///
    /// # Role conventions for `initial_messages`
    ///
    /// The history is primarily `User`/`Assistant` turns in conversation
    /// order. When replaying tool history, `ToolUse` blocks belong in
    /// `Assistant` messages and each matching `ToolResult` block in the
    /// immediately following `User` message — providers reject requests
    /// whose `tool_use`/`tool_result` ids are not paired this way, and the
    /// runtime does not repair an unpaired history for you.
    ///
    /// A `System` message may also appear inside `initial_messages`; how it
    /// reaches the wire is protocol-defined: Anthropic Messages-protocol
    /// adapters extract every `System`-role message (including the one
    /// generated from `config.system_prompt`) and merge them into the
    /// top-level `system` request field, while Chat-protocol adapters keep
    /// them in position in the message stream. Neither treats them as a
    /// conversation turn.
    ///
    /// # Division of labor with `resume_with_input`
    ///
    /// This entry point starts a **new** run — fresh [`RunId`], step 0,
    /// fresh budget — and does not touch any `SessionStore`.
    /// [`AgentRun::resume_with_input`] is the complementary path for
    /// continuing a persisted run: it replays the snapshot's message
    /// history unchanged (same `RunId`, restored step and budget) and
    /// appends the new input on top.
    pub fn start_with_messages(
        config: AgentConfig,
        initial_messages: Vec<crate::model::Message>,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        Self::start_with_bus(
            config,
            input.into_blocks(),
            initial_messages,
            model,
            registry,
            ApprovalBus::default(),
            ChildRunRegistry::default(),
            vec![],
        )
    }

    /// Like [`AgentRun::start_with_messages`], but with watchers active before
    /// the first runtime event. See [`AgentRun::start_with_watchers`].
    #[allow(clippy::too_many_arguments)] // justified: mirrors start_with_messages + watchers
    pub fn start_with_messages_and_watchers(
        config: AgentConfig,
        initial_messages: Vec<crate::model::Message>,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        watchers: Vec<(Arc<dyn Watcher>, usize)>,
    ) -> Result<(RunHandle, EventReceiver), ConfigError> {
        for (_, capacity) in &watchers {
            if *capacity == 0 {
                return Err(ConfigError::InvalidWatcherCapacity(0));
            }
        }
        Ok(Self::start_with_bus(
            config,
            input.into_blocks(),
            initial_messages,
            model,
            registry,
            ApprovalBus::default(),
            ChildRunRegistry::default(),
            watchers,
        ))
    }

    #[allow(clippy::too_many_arguments)] // justified: internal API collecting all run params
    pub(crate) fn start_with_bus(
        mut config: AgentConfig,
        input: Vec<crate::model::ContentBlock>,
        initial_messages: Vec<crate::model::Message>,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        approval_bus: ApprovalBus,
        child_registry: ChildRunRegistry,
        watchers: Vec<(Arc<dyn Watcher>, usize)>,
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
            child_registry: child_registry.clone(),
            resume: None,
            initial_messages,
            initial_event_subs: vec![],
            watcher_wave_bus: None,
        };
        supervisor::spawn_supervised(
            run_id,
            args,
            approval_bus,
            child_registry,
            event_rx,
            watchers,
        )
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
        let child_registry = ChildRunRegistry::default();
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
            child_registry: child_registry.clone(),
            resume: Some(resume),
            initial_messages: vec![],
            initial_event_subs: vec![],
            watcher_wave_bus: None,
        };
        Ok(supervisor::spawn_supervised(
            run_id,
            args,
            approval_bus,
            child_registry,
            event_rx,
            vec![],
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
        let child_registry = ChildRunRegistry::default();
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
            child_registry: child_registry.clone(),
            resume: Some(resume),
            initial_messages: vec![],
            initial_event_subs: vec![],
            watcher_wave_bus: None,
        };
        Ok(supervisor::spawn_supervised(
            run_id,
            args,
            approval_bus,
            child_registry,
            event_rx,
            vec![],
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
