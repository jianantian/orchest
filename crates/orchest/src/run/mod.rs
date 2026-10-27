//! Agent run orchestration, split into single-responsibility modules.

pub(crate) mod action_arbitration;
pub(crate) mod actor;
pub(crate) mod compaction;
pub(crate) mod config;
pub(crate) mod handle;
pub(crate) mod helpers;
pub mod llm_watcher;
pub mod request;
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
    ChildCompletionError, ChildRunHandle, ChildRunOutcome, ChildRunRegistry, EventReceiver,
    RunHandle,
};
pub use llm_watcher::LlmWatcher;
pub use request::{
    ApprovalScope, PendingRequest, RequestBus, RequestDecision, RequestError, RequestId,
    RequestKind, RequestResolution, RequestResponse, RequestTarget,
};
pub use retry::{BackoffStrategy, RetryPolicy};
pub use watcher::{Watcher, WatcherAction};

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;

use crate::session::{SessionError, SessionLog};

use actor::{AgentRunArgs, ResumeState, SessionBinding};

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
    /// fresh budget. With a session store configured it opens a new session
    /// log and fails the run if the session already holds events.
    /// [`AgentRun::resume_with_input`] is the complementary path for
    /// continuing a persisted session: it projects the stored log (same
    /// `RunId`, restored step and budget) and appends the new input on top.
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
            RequestBus::default(),
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
            RequestBus::default(),
            ChildRunRegistry::default(),
            watchers,
        ))
    }

    #[allow(clippy::too_many_arguments)] // justified: internal API collecting all run params
    pub(crate) fn start_with_bus(
        config: AgentConfig,
        input: Vec<crate::model::ContentBlock>,
        initial_messages: Vec<crate::model::Message>,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        request_bus: RequestBus,
        child_registry: ChildRunRegistry,
        watchers: Vec<(Arc<dyn Watcher>, usize)>,
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
            request_bus: request_bus.clone(),
            child_registry: child_registry.clone(),
            resume: None,
            initial_messages,
            initial_event_subs: vec![],
            watcher_wave_bus: None,
        };
        supervisor::spawn_supervised(supervisor::SpawnSupervised {
            run_id,
            args,
            request_bus,
            child_registry,
            event_rx,
            initial_watchers: watchers,
        })
    }

    /// Resumes the run recorded in `config`'s session log exactly as it left
    /// off: the log is read from `config.session_store` under
    /// `config.session_id`, projected into the conversation, step and budget,
    /// and the run continues appending to the same log under the same
    /// [`RunId`].
    ///
    /// This is for continuing an interrupted run (crash, process restart),
    /// not for asking a follow-up question: it does **not** append any new
    /// input. Calling it on a session whose last turn already produced a
    /// final answer just re-invokes the model against unchanged history. For
    /// a follow-up turn, use [`AgentRun::resume_with_input`] instead.
    ///
    /// Nothing but the log is restored. The host supplies the config (hooks,
    /// retry policy, handoffs, store), the model and the tool registry again,
    /// and the runtime does not assume they match the ones the log was
    /// written under.
    ///
    /// Returns [`SessionError::NotConfigured`] when `config` has no session
    /// store or session id, and [`SessionError::NotFound`] when the store
    /// holds no events for the session.
    pub async fn resume(
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> Result<(RunHandle, EventReceiver), SessionError> {
        Self::resume_inner(config, None, model, registry).await
    }

    /// Resumes the run recorded in `config`'s session log and records `input`
    /// as a new user turn — the follow-up-question path (e.g. "continue this
    /// session with a new question").
    ///
    /// Unlike [`AgentRun::resume`], which continues the log unchanged, this
    /// appends `input` to the log before the first step, so the model is
    /// invoked against the history plus the new turn. Errors are the same as
    /// for `resume`.
    pub async fn resume_with_input(
        config: AgentConfig,
        input: RunInput,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> Result<(RunHandle, EventReceiver), SessionError> {
        Self::resume_inner(config, Some(input.into_blocks()), model, registry).await
    }

    async fn resume_inner(
        config: AgentConfig,
        input: Option<Vec<crate::model::ContentBlock>>,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> Result<(RunHandle, EventReceiver), SessionError> {
        let binding = SessionBinding::from_config(&config).ok_or(SessionError::NotConfigured)?;
        let log = load_session_log(&binding).await?;
        let run_id = log.projection().run_id.unwrap_or_default();
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let request_bus = RequestBus::default();
        let child_registry = ChildRunRegistry::default();
        let args = AgentRunArgs {
            run_id,
            config,
            input: vec![],
            model,
            registry,
            event_tx,
            request_bus: request_bus.clone(),
            child_registry: child_registry.clone(),
            resume: Some(ResumeState { log, input }),
            initial_messages: vec![],
            initial_event_subs: vec![],
            watcher_wave_bus: None,
        };
        Ok(supervisor::spawn_supervised(supervisor::SpawnSupervised {
            run_id,
            args,
            request_bus,
            child_registry,
            event_rx,
            initial_watchers: vec![],
        }))
    }
}

/// Reads a session's whole log from its store.
pub(crate) async fn load_session_log(binding: &SessionBinding) -> Result<SessionLog, SessionError> {
    binding.store.get_meta(&binding.session_id).await?;
    let events = binding.store.read(&binding.session_id, 1).await?;
    if events.is_empty() {
        return Err(SessionError::NotFound(binding.session_id.clone()));
    }
    SessionLog::from_events(events)
}

#[cfg(test)]
mod checkpoint_tests;
#[cfg(test)]
mod permission_tests;
#[cfg(test)]
mod tests;
