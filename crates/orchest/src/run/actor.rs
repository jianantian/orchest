//! WorkerActor: Ractor-based agent run actor (mode B: self-message step).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::stream::{FuturesUnordered, StreamExt};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::budget::{BudgetConfig, BudgetGuard};
use crate::events::{ApprovalContext, RuntimeEvent};
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelResponse, ModelStreamChunk, Role, StopReason,
};
use crate::telemetry;
use crate::tool::code_exec::CodeExecutionMcpServer;
use crate::tool::registry::ToolRegistry;
use crate::tool::search::SearchToolsTool;
use crate::tool::{
    ErrorKind, RetryHint, Tool, ToolCall, ToolContext, ToolDef, ToolError, ToolExecutionMode,
    ToolMetadata, ToolOutput, ToolParallelism, ToolSource,
};

use super::compaction::maybe_compact_context;
use super::config::{AgentConfig, RunId, ToolExecutionPolicy};
use super::handle::ApprovalBus;
use super::helpers::{append_searched_tool_defs, connect_mcp_servers, truncate_output};
use super::skills::register_skills;
use super::tool_exec::{poll_async_job, tool_error_result, tool_skipped_by_hook_result};
use super::webhook::{start_webhook_server, WebhookRuntime};

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(3600);
const EVENT_SEND_TIMEOUT: Duration = Duration::from_millis(500);
const TOOL_RETRY_MAX_ATTEMPTS: u32 = 3;
const TOOL_RETRY_BASE_DELAY: Duration = Duration::from_millis(10);

// ── Message enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct SteerCmd {
    pub instruction: String,
}

#[derive(Debug, Clone)]
pub(crate) struct InjectCmd {
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CancelCmd {
    pub reason: Option<String>,
}

pub(crate) enum AgentMsg {
    RunStep,
    Subscribe(mpsc::Sender<RuntimeEvent>),
    Steer(SteerCmd),
    Inject(InjectCmd),
    Cancel(CancelCmd),
}

// ── Run state ─────────────────────────────────────────────────────────────────

pub(crate) struct AgentRunState {
    pub run_id: RunId,
    pub config: AgentConfig,
    pub model: Arc<dyn ModelAdapter>,
    pub unfiltered_registry: ToolRegistry,
    pub registry: ToolRegistry,
    pub messages: Vec<Message>,
    pub tool_defs: Vec<ToolDef>,
    pub step: u32,
    pub budget: BudgetGuard,
    pub last_compaction_step: Option<u32>,
    pub cancelled: bool,
    pub run_hook_ctx: crate::hook::RunHookContext,
    pub approval_bus: ApprovalBus,
    /// Subscriber list; index 0 is the primary (blocking send), rest use try_send.
    pub event_subs: Vec<mpsc::Sender<RuntimeEvent>>,
    pub webhook_runtime: Option<WebhookRuntime>,
    pub repeated_failures: HashMap<(String, ErrorKind), Vec<ToolError>>,
}

struct PreparedHandoffTransition {
    messages: Vec<Message>,
    config: AgentConfig,
    registry: ToolRegistry,
    tool_defs: Vec<ToolDef>,
    budget: BudgetGuard,
}

impl AgentRunState {
    /// Refresh the terminal fields of `run_hook_ctx` before calling
    /// `on_run_end` or `on_run_error`. Must be called at all 7 exit points.
    fn refresh_terminal_hook_ctx(&mut self, step: u32) {
        self.run_hook_ctx.step = step;
        self.run_hook_ctx.budget_used = self.budget.usage().clone();
        self.run_hook_ctx.final_messages = self.messages.clone();
        self.run_hook_ctx.active_config = Some(self.config.clone());
    }
}

// ── Constructor args ──────────────────────────────────────────────────────────

/// Resume payload: injected by `AgentRun::resume` to restore prior run state.
#[derive(Clone)]
pub(crate) struct ResumeState {
    pub messages: Vec<crate::model::Message>,
    pub step: u32,
    pub budget_used: crate::budget::BudgetUsage,
}

#[derive(Clone)]
pub(crate) struct AgentRunArgs {
    pub run_id: RunId,
    pub config: AgentConfig,
    pub input: Vec<ContentBlock>,
    pub model: Arc<dyn ModelAdapter>,
    pub registry: ToolRegistry,
    pub event_tx: mpsc::Sender<RuntimeEvent>,
    pub approval_bus: ApprovalBus,
    /// `None` on a fresh start; `Some` when resuming from a persisted snapshot.
    pub resume: Option<ResumeState>,
    /// Extra messages to prepend (between system prompt and user input) on a fresh start.
    pub initial_messages: Vec<crate::model::Message>,
}

// ── WorkerActor ───────────────────────────────────────────────────────────────

pub(crate) struct WorkerActor;

impl Actor for WorkerActor {
    type Msg = AgentMsg;
    type State = AgentRunState;
    type Arguments = AgentRunArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<AgentMsg>,
        args: AgentRunArgs,
    ) -> Result<AgentRunState, ActorProcessingErr> {
        let AgentRunArgs {
            run_id,
            mut config,
            input,
            model,
            mut registry,
            event_tx,
            approval_bus,
            resume,
            initial_messages,
        } = args;
        let event_subs = vec![event_tx];

        emit(&event_subs, RuntimeEvent::RunStarted { run_id }).await;

        let agent_name = config.system_prompt[..config.system_prompt.len().min(60)].to_string();
        let mut run_hook_ctx = crate::hook::RunHookContext {
            run_id,
            agent_name,
            step: 0,
            budget_used: crate::budget::BudgetUsage::default(),
            final_messages: vec![],
            active_config: None,
        };
        crate::hook::runner::run_on_run_start(
            &config.hooks,
            &mut run_hook_ctx,
            primary(&event_subs),
        )
        .await;

        let webhook_runtime = if config.runtime.webhook_enabled {
            match start_webhook_server().await {
                Ok(rt) => Some(rt),
                Err(error) => {
                    emit(&event_subs, RuntimeEvent::RuntimeWarning { message: error }).await;
                    None
                }
            }
        } else {
            None
        };

        if let Err(error) = connect_mcp_servers(&config, &mut registry).await {
            return Ok(fail_pre_start(
                &myself,
                &event_subs,
                run_id,
                config,
                model,
                registry,
                approval_bus,
                run_hook_ctx,
                error.message,
            )
            .await);
        }

        if config.runtime.code_execution_enabled {
            if let Err(error) =
                register_code_execution_tools(&config, &mut registry, &event_subs).await
            {
                return Ok(fail_pre_start(
                    &myself,
                    &event_subs,
                    run_id,
                    config,
                    model,
                    registry,
                    approval_bus,
                    run_hook_ctx,
                    error.to_string(),
                )
                .await);
            }
        }

        if let Some(ref skills_dir) = config.skills.dir.clone() {
            if let Err(error) = register_skills(
                skills_dir,
                &config.skills.allowed,
                &mut registry,
                primary(&event_subs),
            )
            .await
            {
                return Ok(fail_pre_start(
                    &myself,
                    &event_subs,
                    run_id,
                    config,
                    model,
                    registry,
                    approval_bus,
                    run_hook_ctx,
                    format!("skill loading failed: {error}"),
                )
                .await);
            }
        }

        for handoff in config.handoffs.drain(..) {
            let tool: Arc<dyn Tool> =
                Arc::new(crate::tool::handoff_tool::HandoffTool::new(handoff));
            if let Err(e) = registry.register(tool) {
                emit(
                    &event_subs,
                    RuntimeEvent::RuntimeWarning {
                        message: format!("failed to register handoff tool: {e}"),
                    },
                )
                .await;
            }
        }

        let unfiltered_registry = registry.clone();
        let mut registry = registry.filter_by_allowed(&config.runtime.allowed_tools);
        if let Err(error) = registry.validate_metadata_links() {
            return Ok(fail_pre_start(
                &myself,
                &event_subs,
                run_id,
                config,
                model,
                registry,
                approval_bus,
                run_hook_ctx,
                format!("tool metadata validation failed: {error}"),
            )
            .await);
        }

        let (messages, initial_step, initial_budget_used) = if let Some(rs) = resume {
            (rs.messages, rs.step, Some(rs.budget_used))
        } else {
            let mut msgs = vec![Message {
                role: Role::System,
                content: vec![ContentBlock::Text(config.system_prompt.clone())],
            }];
            msgs.extend(initial_messages);
            msgs.push(Message {
                role: Role::User,
                content: input,
            });
            (msgs, 0, None)
        };

        let all_tool_defs = registry.list();
        let tool_defs = if config.runtime.tool_search_enabled {
            let search_tool = Arc::new(SearchToolsTool::new(all_tool_defs));
            let search_def = ToolDef {
                name: search_tool.name().to_string(),
                description: search_tool.description().to_string(),
                input_schema: search_tool.input_schema().clone(),
            };
            if let Err(error) = registry.register(search_tool) {
                emit(
                    &event_subs,
                    RuntimeEvent::RunFailed {
                        error: error.to_string(),
                    },
                )
                .await;
                myself.cast(AgentMsg::RunStep).ok();
                return Ok(failed_state(
                    run_id,
                    config,
                    model,
                    registry,
                    approval_bus,
                    event_subs,
                    run_hook_ctx,
                ));
            }
            vec![search_def]
        } else {
            all_tool_defs
        };

        let budget = if let Some(used) = initial_budget_used {
            BudgetGuard::with_usage(config.budget.clone(), used)
        } else {
            BudgetGuard::new(config.budget.clone())
        };

        myself.cast(AgentMsg::RunStep)?;

        Ok(AgentRunState {
            run_id,
            config,
            model,
            unfiltered_registry,
            registry,
            messages,
            tool_defs,
            step: initial_step,
            budget,
            last_compaction_step: None,
            cancelled: false,
            run_hook_ctx,
            approval_bus,
            event_subs,
            webhook_runtime,
            repeated_failures: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<AgentMsg>,
        msg: AgentMsg,
        state: &mut AgentRunState,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            AgentMsg::RunStep => {
                if state.cancelled {
                    myself.stop(None);
                    return Ok(());
                }
                let should_continue = run_one_step(state).await;
                if should_continue {
                    myself.cast(AgentMsg::RunStep)?;
                } else {
                    myself.stop(None);
                }
            }
            AgentMsg::Subscribe(tx) => {
                state.event_subs.push(tx);
            }
            AgentMsg::Steer(cmd) => {
                state.messages.push(Message {
                    role: Role::System,
                    content: vec![ContentBlock::Text(cmd.instruction)],
                });
            }
            AgentMsg::Inject(cmd) => {
                state.messages.push(Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text(cmd.message)],
                });
            }
            AgentMsg::Cancel(cmd) => {
                if !state.cancelled {
                    state.cancelled = true;
                    emit(
                        &state.event_subs,
                        RuntimeEvent::RunAborted { reason: cmd.reason },
                    )
                    .await;
                }
                myself.stop(None);
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<AgentMsg>,
        state: &mut AgentRunState,
    ) -> Result<(), ActorProcessingErr> {
        state.approval_bus.cancel(state.run_id).await;
        Ok(())
    }
}

async fn register_code_execution_tools(
    config: &AgentConfig,
    registry: &mut ToolRegistry,
    event_subs: &[mpsc::Sender<RuntimeEvent>],
) -> Result<(), ToolError> {
    let tools = CodeExecutionMcpServer::tools(config.runtime.code_execution_executor.clone())?;
    for tool in tools {
        if let Err(error) = registry.register(tool) {
            emit(
                event_subs,
                RuntimeEvent::RuntimeWarning {
                    message: format!("failed to register code execution tool: {error}"),
                },
            )
            .await;
        }
    }
    Ok(())
}

// ── Per-step logic ────────────────────────────────────────────────────────────

/// Runs `after_tool` hooks for the just-pushed tool result and applies any
/// rewrite the hooks made to `ctx.tool_output`. Returns `Err(reason)` if a hook
/// aborted the run. The current output is read from (and written back to) the
/// last entry of `tool_results`.
#[allow(clippy::too_many_arguments)] // justified: per-call context for the after_tool hook chain
async fn finalize_after_tool(
    hooks: &[std::sync::Arc<dyn crate::hook::Hook>],
    subs: &[mpsc::Sender<RuntimeEvent>],
    run_id: RunId,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_meta: &crate::tool::ToolMetadata,
    tool_results: &mut [ContentBlock],
) -> Result<(), String> {
    let current_output = tool_results.last().and_then(|b| match b {
        ContentBlock::ToolResult { content, .. } => Some(content.clone()),
        _ => None,
    });
    let mut ctx = crate::hook::ToolHookContext {
        run_id,
        tool_name: tool_name.to_string(),
        tool_input: tool_input.clone(),
        tool_metadata: tool_meta.clone(),
        tool_output: current_output.clone(),
    };
    if let crate::hook::HookAction::Abort(reason) =
        crate::hook::runner::run_after_tool(hooks, &mut ctx, primary(subs)).await
    {
        return Err(reason);
    }
    if let Some(modified) = ctx.tool_output {
        if Some(&modified) != current_output.as_ref() {
            if let Some(ContentBlock::ToolResult { content, .. }) = tool_results.last_mut() {
                *content = modified;
            }
        }
    }
    Ok(())
}

fn should_retry_tool_error(error: &ToolError, attempt: u32) -> bool {
    if attempt >= TOOL_RETRY_MAX_ATTEMPTS {
        return false;
    }

    match error.retry {
        RetryHint::Safe => error.kind == ErrorKind::Transient,
        RetryHint::Caution => true,
        RetryHint::Unsafe => false,
    }
}

fn budget_violation_kind(violation: &crate::budget::BudgetViolation) -> &'static str {
    match violation {
        crate::budget::BudgetViolation::MaxTokensExceeded => "tokens",
        crate::budget::BudgetViolation::MaxToolCallsExceeded => "tool_calls",
        crate::budget::BudgetViolation::MaxDurationExceeded => "duration",
        crate::budget::BudgetViolation::MaxCostExceeded => "cost",
    }
}

fn tool_retry_delay(next_attempt: u32) -> Duration {
    let exponent = next_attempt.saturating_sub(2).min(8);
    TOOL_RETRY_BASE_DELAY.saturating_mul(1 << exponent)
}

struct RetryApprovalRequest<'a> {
    approval_bus: &'a ApprovalBus,
    subs: &'a [mpsc::Sender<RuntimeEvent>],
    run_id: RunId,
    tool_call: &'a ToolCall,
    attempt: u32,
    previous_error: &'a ToolError,
}

async fn request_retry_approval(request: RetryApprovalRequest<'_>) -> bool {
    let context = ApprovalContext::RetryAfterFailure {
        attempt: request.attempt,
        previous_error: request.previous_error.clone(),
    };
    let approval_rx = request.approval_bus.request(request.run_id).await;
    let approval_started = Instant::now();
    emit(
        request.subs,
        RuntimeEvent::ApprovalRequested {
            tool_call: request.tool_call.clone(),
            context: context.clone(),
        },
    )
    .await;

    let (approved, approval_status) =
        match tokio::time::timeout(APPROVAL_TIMEOUT, approval_rx).await {
            Ok(result) => {
                let approved = result.unwrap_or(false);
                (approved, if approved { "granted" } else { "denied" })
            }
            Err(_) => (false, "timeout"),
        };
    request.approval_bus.cancel(request.run_id).await;
    telemetry::record_approval(approval_status, approval_started.elapsed());

    if approved {
        emit(
            request.subs,
            RuntimeEvent::ApprovalGranted {
                tool_call: request.tool_call.clone(),
                context,
            },
        )
        .await;
    } else {
        emit(
            request.subs,
            RuntimeEvent::ApprovalDenied {
                tool_call: request.tool_call.clone(),
                context,
            },
        )
        .await;
    }

    approved
}

async fn record_repeated_failure(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    tool_name: &str,
    error: &ToolError,
) -> Result<(), String> {
    let key = (tool_name.to_string(), error.kind);
    let history = state.repeated_failures.entry(key).or_default();
    history.push(error.clone());

    let threshold = state.config.runtime.repeated_failure.threshold;
    if history.len() < threshold {
        return Ok(());
    }

    let ctx = crate::hook::RepeatedFailureHookContext {
        run_id: state.run_id,
        tool_name: tool_name.to_string(),
        error_kind: error.kind,
        error_history: history.clone(),
        count: history.len(),
    };

    match crate::hook::runner::run_on_repeated_failure(&state.config.hooks, &ctx, primary(subs))
        .await
    {
        crate::hook::HookAction::Abort(reason) => Err(reason),
        crate::hook::HookAction::Continue
        | crate::hook::HookAction::Skip
        | crate::hook::HookAction::Reject(_) => Ok(()),
    }
}

async fn check_step_limits(state: &mut AgentRunState, subs: &[mpsc::Sender<RuntimeEvent>]) -> bool {
    let step = state.step;

    if step >= state.config.runtime.max_steps {
        state.refresh_terminal_hook_ctx(step);
        crate::hook::runner::run_on_run_error(
            &state.config.hooks,
            &state.run_hook_ctx,
            "max_steps_reached",
            primary(subs),
        )
        .await;
        emit(
            subs,
            RuntimeEvent::RunFailed {
                error: "max_steps_reached".into(),
            },
        )
        .await;
        return false;
    }

    if let Some(violation) = state.budget.check() {
        telemetry::record_budget_exceeded(budget_violation_kind(&violation));
        emit(
            subs,
            RuntimeEvent::BudgetWarning {
                used: state.budget.usage().clone(),
                limit: state.budget.config().clone(),
            },
        )
        .await;
        let error = format!("budget_exceeded: {violation}");
        state.refresh_terminal_hook_ctx(step);
        crate::hook::runner::run_on_run_error(
            &state.config.hooks,
            &state.run_hook_ctx,
            &error,
            primary(subs),
        )
        .await;
        emit(subs, RuntimeEvent::RunFailed { error }).await;
        return false;
    }

    true
}

async fn call_model_phase(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
) -> Option<ModelResponse> {
    let step = state.step;
    let run_id = state.run_id;

    emit(subs, RuntimeEvent::ModelCallStarted { step }).await;

    // Snapshot the message history before this step's hooks.
    // Each before_model invocation (including retries) receives a fresh clone
    // of this snapshot, so hook injections are ephemeral: they are passed to
    // the model for this call but never accumulate in the durable conversation
    // history stored in state.messages.
    let pre_step_messages = state.messages.clone();
    let mut retry_attempt: u32 = 0;
    loop {
        // Build per-attempt call context from the clean pre-step snapshot.
        // Hook mutations stay inside call_messages; state.messages is untouched.
        let call_messages = {
            let mut model_ctx = crate::hook::ModelHookContext {
                run_id,
                messages: pre_step_messages.clone(),
                model_spec: state.config.model.spec.clone(),
                response: None,
            };
            match crate::hook::runner::run_before_model(
                &state.config.hooks,
                &mut model_ctx,
                primary(subs),
            )
            .await
            {
                crate::hook::ModelHookAction::Abort(reason) => {
                    state.refresh_terminal_hook_ctx(step);
                    crate::hook::runner::run_on_run_error(
                        &state.config.hooks,
                        &state.run_hook_ctx,
                        &reason,
                        primary(subs),
                    )
                    .await;
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return None;
                }
                crate::hook::ModelHookAction::Continue => {}
            }
            model_ctx.messages
        };

        if !validate_context_window(state, subs, &call_messages).await {
            return None;
        }

        let raw_response = dispatch_model_call(state, subs, &call_messages).await;

        match raw_response {
            Ok(response) => {
                return apply_after_model_hooks(state, subs, call_messages, response).await;
            }
            Err(error) => {
                if !handle_model_error_or_retry(state, subs, &error, retry_attempt).await {
                    return None;
                }
                retry_attempt += 1;
            }
        }
    }
}

async fn validate_context_window(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    call_messages: &[Message],
) -> bool {
    let Some(context_window_size) = state.config.model.spec.context_window_size else {
        return true;
    };
    let estimated_tokens = estimate_context_tokens(call_messages, &state.tool_defs);
    if estimated_tokens <= context_window_size {
        return true;
    }

    let error = format!(
        "context window exceeded: estimated {estimated_tokens} tokens, limit {context_window_size}"
    );
    state.refresh_terminal_hook_ctx(state.step);
    crate::hook::runner::run_on_run_error(
        &state.config.hooks,
        &state.run_hook_ctx,
        &error,
        primary(subs),
    )
    .await;
    emit(subs, RuntimeEvent::RunFailed { error }).await;
    false
}

async fn dispatch_model_call(
    state: &AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    call_messages: &[Message],
) -> Result<ModelResponse, crate::model::ModelError> {
    let (stream_tx, mut stream_rx) = mpsc::channel::<ModelStreamChunk>(64);
    let event_tx_clone = primary(subs).clone();
    let forward_task = tokio::spawn(async move {
        while let Some(chunk) = stream_rx.recv().await {
            if tokio::time::timeout(
                EVENT_SEND_TIMEOUT,
                event_tx_clone.send(RuntimeEvent::ModelStreamChunk { delta: chunk }),
            )
            .await
            .is_err()
            {
                tracing::warn!(
                    "primary event subscriber timed out while forwarding model stream chunk"
                );
            }
        }
    });

    let provider = state.model.provider_name().to_string();
    let model = state.model.model_name().to_string();
    let start = Instant::now();
    let span = telemetry::model_complete_span(&provider, &model, true);
    let raw_response = async {
        state
            .model
            .complete(
                call_messages,
                &state.tool_defs,
                &state.config.model.options,
                Some(stream_tx),
            )
            .await
    }
    .instrument(span)
    .await;
    let duration = start.elapsed();
    match &raw_response {
        Ok(response) => {
            telemetry::record_model_success(&provider, &model, duration, &response.usage)
        }
        Err(_) => telemetry::record_model_error(&provider, &model, duration),
    }
    let _ = forward_task.await;
    raw_response
}

async fn apply_after_model_hooks(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    call_messages: Vec<Message>,
    mut response: ModelResponse,
) -> Option<ModelResponse> {
    let mut model_ctx = crate::hook::ModelHookContext {
        run_id: state.run_id,
        messages: call_messages,
        model_spec: state.config.model.spec.clone(),
        response: Some(response.content.clone()),
    };
    if let crate::hook::HookAction::Abort(reason) =
        crate::hook::runner::run_after_model(&state.config.hooks, &mut model_ctx, primary(subs))
            .await
    {
        state.refresh_terminal_hook_ctx(state.step);
        crate::hook::runner::run_on_run_error(
            &state.config.hooks,
            &state.run_hook_ctx,
            &reason,
            primary(subs),
        )
        .await;
        emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
        return None;
    }
    // Read back any rewrite the hook applied to the response.
    if let Some(modified) = model_ctx.response.take() {
        response.content = modified;
    }
    Some(response)
}

async fn handle_model_error_or_retry(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    error: &crate::model::ModelError,
    retry_attempt: u32,
) -> bool {
    let class = super::retry::classify(error);
    let should_retry =
        super::retry::should_retry(&class, retry_attempt, &state.config.retry_policy);
    if should_retry {
        let Some(policy) = state.config.retry_policy.as_ref() else {
            return false;
        };
        let delay = super::retry::compute_delay(retry_attempt, error, policy);
        emit(
            subs,
            RuntimeEvent::ModelRetry {
                attempt: retry_attempt + 1,
                error: error.message.clone(),
                next_delay: delay,
            },
        )
        .await;
        tokio::time::sleep(delay).await;
        return true;
    }

    let error = error.to_string();
    state.refresh_terminal_hook_ctx(state.step);
    crate::hook::runner::run_on_run_error(
        &state.config.hooks,
        &state.run_hook_ctx,
        &error,
        primary(subs),
    )
    .await;
    emit(subs, RuntimeEvent::RunFailed { error }).await;
    false
}

struct PendingHandoff {
    tool_name: String,
    tool_use_id: String,
    result: crate::handoff::HandoffResult,
}

struct ParallelToolCall {
    requested_order: usize,
    tool_call: ToolCall,
    tool: Arc<dyn Tool>,
    metadata: ToolMetadata,
    input: Value,
    source_label: &'static str,
    max_output_tokens: Option<u64>,
}

struct ParallelToolResult {
    requested_order: usize,
    tool_call: ToolCall,
    source_label: &'static str,
    max_output_tokens: Option<u64>,
    result: Result<(ToolOutput, Instant), ToolError>,
}

struct ParallelExecutionContext {
    run_id: RunId,
    run_depth: u32,
    event_tx: mpsc::Sender<RuntimeEvent>,
    webhook_base_url: Option<String>,
    approval_bus: ApprovalBus,
    remaining_budget: BudgetConfig,
    parent_messages: Vec<Message>,
}

/// Execute one outer-loop iteration. Returns true to continue, false to stop.
async fn run_one_step(state: &mut AgentRunState) -> bool {
    let subs = state.event_subs.clone();
    let step = state.step;
    let run_id = state.run_id;

    if !check_step_limits(state, &subs).await {
        return false;
    }

    let Some(response) = call_model_phase(state, &subs).await else {
        return false;
    };

    state.budget.record_model_call(&response.usage);
    telemetry::record_budget_usage(state.budget.usage(), state.budget.config());

    emit(
        &subs,
        RuntimeEvent::ModelCallCompleted {
            tokens: response.usage.clone(),
            option_adjustments: response.option_adjustments.clone(),
        },
    )
    .await;

    maybe_compact_context(
        &state.config,
        &state.model,
        &mut state.messages,
        primary(&subs),
        &mut state.last_compaction_step,
        step,
        &response.usage,
        run_id,
    )
    .await;

    let mut tool_uses = Vec::new();
    let mut text_parts = Vec::new();

    for block in &response.content {
        match block {
            ContentBlock::ToolUse { id, name, input } => {
                tool_uses.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            ContentBlock::Text(t) => {
                text_parts.push(t.clone());
            }
            _ => {}
        }
    }

    match &response.stop_reason {
        StopReason::EndTurn if tool_uses.is_empty() => {
            let output = json!(text_parts.join(""));
            state.refresh_terminal_hook_ctx(step);
            crate::hook::runner::run_on_run_end(
                &state.config.hooks,
                &state.run_hook_ctx,
                primary(&subs),
            )
            .await;
            emit(
                &subs,
                RuntimeEvent::RunCompleted {
                    output,
                    stop_reason: StopReason::EndTurn,
                },
            )
            .await;
            return false;
        }
        StopReason::MaxTokens if tool_uses.is_empty() => {
            let output = json!(text_parts.join(""));
            state.refresh_terminal_hook_ctx(step);
            crate::hook::runner::run_on_run_end(
                &state.config.hooks,
                &state.run_hook_ctx,
                primary(&subs),
            )
            .await;
            emit(
                &subs,
                RuntimeEvent::RunCompleted {
                    output,
                    stop_reason: StopReason::MaxTokens,
                },
            )
            .await;
            return false;
        }
        // Abnormal stop (e.g. ContextWindowExceeded) with no tool calls:
        // falling through to the tool phase would push an empty-content User
        // message and re-call the model on the same context until max_steps,
        // burning tokens and diluting the failure. Fail the run immediately
        // instead, through the same on_run_error path as step-limit failures.
        reason if tool_uses.is_empty() => {
            let error = format!("abnormal_stop_reason: {reason:?}");
            state.refresh_terminal_hook_ctx(step);
            crate::hook::runner::run_on_run_error(
                &state.config.hooks,
                &state.run_hook_ctx,
                &error,
                primary(&subs),
            )
            .await;
            emit(&subs, RuntimeEvent::RunFailed { error }).await;
            return false;
        }
        _ => {}
    }

    run_tool_and_handoff_phase(state, &subs, run_id, &response, &tool_uses).await
}

#[allow(clippy::too_many_lines)] // justified: mechanical extraction of existing tool dispatch flow; narrower helpers follow in later refactors
async fn run_tool_and_handoff_phase(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    run_id: RunId,
    response: &ModelResponse,
    tool_uses: &[ToolCall],
) -> bool {
    state.messages.push(Message {
        role: Role::Assistant,
        content: response.content.clone(),
    });

    let mut tool_results = Vec::new();
    let mut handoff_triggered = false;
    let mut pending_handoff: Option<PendingHandoff> = None;

    if let Some(parallel_results) =
        run_parallel_tool_batch_if_allowed(state, subs, run_id, tool_uses).await
    {
        state.messages.push(Message {
            role: Role::Tool,
            content: parallel_results,
        });
        return true;
    }

    for tool_call in tool_uses {
        if let Some(error) = deferred_tool_exposure_error(state, &tool_call.name) {
            emit(
                subs,
                RuntimeEvent::ToolCallFailed {
                    tool: tool_call.name.clone(),
                    error: error.clone(),
                },
            )
            .await;
            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: tool_error_result(&error),
            });
            continue;
        }

        let tool = match state.registry.get(&tool_call.name) {
            Some(t) => t,
            None => {
                let error = if state.unfiltered_registry.contains(&tool_call.name) {
                    ToolError::fatal("tool not allowed").with_code("NOT_ALLOWED")
                } else {
                    ToolError::fatal(format!("tool '{}' not found", tool_call.name))
                        .with_code("NOT_FOUND")
                };
                emit(
                    subs,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_call.name.clone(),
                        error: error.clone(),
                    },
                )
                .await;
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: tool_error_result(&error),
                });
                continue;
            }
        };

        let tool_meta = tool.metadata().clone();
        let source_label = match &tool_meta.source {
            crate::tool::ToolSource::Builtin => "builtin",
            crate::tool::ToolSource::InProcess => "in_process",
            crate::tool::ToolSource::McpServer { .. } => "mcp_server",
            crate::tool::ToolSource::Skill { .. } => "skill",
        };

        // before_tool runs BEFORE approval so that approval (and execution) act on
        // the final, hook-modified input — never the pre-modification input.
        let mut tool_hook_ctx = crate::hook::ToolHookContext {
            run_id,
            tool_name: tool_call.name.clone(),
            tool_input: tool_call.input.clone(),
            tool_metadata: tool_meta.clone(),
            tool_output: None,
        };
        match crate::hook::runner::run_before_tool(
            &state.config.hooks,
            &mut tool_hook_ctx,
            primary(subs),
        )
        .await
        {
            crate::hook::HookAction::Skip => {
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: tool_skipped_by_hook_result(),
                });
                state.budget.record_tool_call();
                continue;
            }
            crate::hook::HookAction::Reject(reason) => {
                let error = ToolError::fatal(reason).with_code("HOOK_REJECTED");
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: tool_error_result(&error),
                });
                state.budget.record_tool_call();
                continue;
            }
            crate::hook::HookAction::Abort(reason) => {
                emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                return false;
            }
            crate::hook::HookAction::Continue => {}
        }
        let tool_input = tool_hook_ctx.tool_input;

        // Effective call carries the hook-modified input for approval / started events.
        let effective_call = ToolCall {
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            input: tool_input.clone(),
        };

        if state.config.runtime.should_approve(tool.metadata()) {
            let approval_context = approval_context_for(tool.metadata());
            let approval_rx = state.approval_bus.request(run_id).await;
            let approval_started = Instant::now();
            emit(
                subs,
                RuntimeEvent::ApprovalRequested {
                    tool_call: effective_call.clone(),
                    context: approval_context.clone(),
                },
            )
            .await;

            let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, approval_rx).await {
                Ok(result) => result.unwrap_or(false),
                Err(_) => {
                    telemetry::record_approval("timeout", approval_started.elapsed());
                    emit(
                        subs,
                        RuntimeEvent::RunFailed {
                            error: "approval_timeout".into(),
                        },
                    )
                    .await;
                    return false;
                }
            };
            state.approval_bus.cancel(run_id).await;
            telemetry::record_approval(
                if approved { "granted" } else { "denied" },
                approval_started.elapsed(),
            );

            if approved {
                emit(
                    subs,
                    RuntimeEvent::ApprovalGranted {
                        tool_call: effective_call.clone(),
                        context: approval_context,
                    },
                )
                .await;
            } else {
                let error =
                    ToolError::fatal("tool call denied by user").with_code("APPROVAL_DENIED");
                emit(
                    subs,
                    RuntimeEvent::ApprovalDenied {
                        tool_call: effective_call.clone(),
                        context: approval_context,
                    },
                )
                .await;
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: tool_error_result(&error),
                });
                continue;
            }
        }

        if let Some(max) = state.config.budget.max_tool_calls {
            if state.budget.usage().tool_calls_used >= max {
                let error =
                    ToolError::fatal("tool call budget exceeded").with_code("BUDGET_EXCEEDED");
                emit(
                    subs,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_call.name.clone(),
                        error: error.clone(),
                    },
                )
                .await;
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: tool_error_result(&error),
                });
                continue;
            }
        }
        state.budget.record_tool_call();

        let metadata_timeout = tool_meta.timeout;
        let max_output_tokens = tool_meta.max_output_tokens;
        let mut attempt = 1;
        let result = loop {
            if attempt > 1 {
                if let Some(max) = state.config.budget.max_tool_calls {
                    if state.budget.usage().tool_calls_used >= max {
                        let error = ToolError::fatal("tool call budget exceeded")
                            .with_code("BUDGET_EXCEEDED");
                        emit(
                            subs,
                            RuntimeEvent::ToolCallFailed {
                                tool: tool_call.name.clone(),
                                error: error.clone(),
                            },
                        )
                        .await;
                        break Err(error);
                    }
                }
            }
            if attempt > 1 {
                state.budget.record_tool_call();
            }

            // ToolCallStarted fires for each committed execution attempt, after
            // before_tool, approval, and budget checks all pass.
            emit(
                subs,
                RuntimeEvent::ToolCallStarted {
                    tool: tool_call.name.clone(),
                    metadata: tool_meta.clone(),
                    input: tool_input.clone(),
                },
            )
            .await;

            let _tool_span = telemetry::tool_execute_span(&tool_call.name, source_label);

            let parent_messages = if tool.needs_parent_context() {
                state.messages.clone()
            } else {
                vec![]
            };
            let ctx = ToolContext {
                run_id,
                run_depth: state.config.runtime.run_depth,
                tool_call_id: tool_call.id.clone(),
                event_tx: Some(primary(subs).clone()),
                webhook_base_url: state.webhook_runtime.as_ref().map(|rt| rt.base_url.clone()),
                approval_bus: state.approval_bus.clone(),
                remaining_budget: state.budget.remaining_config(),
                parent_messages,
            };

            let start_time = Instant::now();
            let execute_fut = tool.execute(tool_input.clone(), &ctx);
            let (attempt_result, timed_out) = if let Some(timeout) = metadata_timeout {
                match tokio::time::timeout(timeout, execute_fut).await {
                    Ok(result) => (result, false),
                    Err(_) => (
                        Err(ToolError::transient("tool execution timed out").with_code("TIMEOUT")),
                        true,
                    ),
                }
            } else {
                (execute_fut.await, false)
            };

            match attempt_result {
                Ok(output) => break Ok((output, start_time)),
                Err(error) => {
                    let duration = start_time.elapsed();
                    emit(
                        subs,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: error.clone(),
                        },
                    )
                    .await;
                    if timed_out {
                        telemetry::record_tool_timeout(&tool_call.name, source_label, duration);
                    } else {
                        telemetry::record_tool_error(&tool_call.name, source_label, duration);
                    }

                    if !should_retry_tool_error(&error, attempt) {
                        break Err(error);
                    }

                    let next_attempt = attempt + 1;
                    if let Some(max) = state.config.budget.max_tool_calls {
                        if state.budget.usage().tool_calls_used >= max {
                            let budget_error = ToolError::fatal("tool call budget exceeded")
                                .with_code("BUDGET_EXCEEDED");
                            emit(
                                subs,
                                RuntimeEvent::ToolCallFailed {
                                    tool: tool_call.name.clone(),
                                    error: budget_error.clone(),
                                },
                            )
                            .await;
                            break Err(budget_error);
                        }
                    }

                    if error.retry == RetryHint::Caution {
                        let approved = request_retry_approval(RetryApprovalRequest {
                            approval_bus: &state.approval_bus,
                            subs,
                            run_id,
                            tool_call: &effective_call,
                            attempt: next_attempt,
                            previous_error: &error,
                        })
                        .await;
                        if !approved {
                            break Err(error);
                        }
                    }

                    let next_delay = tool_retry_delay(next_attempt);
                    emit(
                        subs,
                        RuntimeEvent::ToolCallRetry {
                            tool: tool_call.name.clone(),
                            attempt: next_attempt,
                            previous_error: error,
                            next_delay,
                        },
                    )
                    .await;
                    tokio::time::sleep(next_delay).await;
                    attempt = next_attempt;
                }
            }
        };

        match result {
            Ok((ToolOutput::Immediate(value), start_time)) => {
                let mut value = value;
                if let Some(max_tokens) = max_output_tokens {
                    value = truncate_output(value, max_tokens);
                }
                if state.config.runtime.tool_search_enabled && tool_call.name == "search_tools" {
                    append_searched_tool_defs(&mut state.tool_defs, &value);
                }
                let duration = start_time.elapsed();
                emit(
                    subs,
                    RuntimeEvent::ToolCallCompleted {
                        tool: tool_call.name.clone(),
                        output: value.clone(),
                        duration,
                    },
                )
                .await;
                telemetry::record_tool_success(&tool_call.name, source_label, duration);
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: value,
                });
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
            Ok((
                ToolOutput::Structured {
                    model_output,
                    details,
                    external_usage,
                },
                start_time,
            )) => {
                if let Some(usage) = external_usage {
                    state.budget.record_external_usage(&usage);
                }
                let mut model_output = model_output;
                let mut details = details;
                if let Some(max_tokens) = max_output_tokens {
                    model_output = truncate_output(model_output, max_tokens);
                    details = truncate_output(details, max_tokens);
                }
                let duration = start_time.elapsed();
                emit(
                    subs,
                    RuntimeEvent::ToolCallCompleted {
                        tool: tool_call.name.clone(),
                        output: details,
                        duration,
                    },
                )
                .await;
                telemetry::record_tool_success(&tool_call.name, source_label, duration);
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: model_output,
                });
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
            Ok((ToolOutput::Handoff(result), _start_time)) => {
                if handoff_triggered {
                    let error = ToolError::fatal("Only one handoff per turn is allowed")
                        .with_code("HANDOFF_ALREADY_TRIGGERED");
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: tool_error_result(&error),
                    });
                } else {
                    handoff_triggered = true;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"result": result.transfer_message}),
                    });
                    pending_handoff = Some(PendingHandoff {
                        tool_name: tool_call.name.clone(),
                        tool_use_id: tool_call.id.clone(),
                        result: *result,
                    });
                }
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
                continue;
            }
            Ok((ToolOutput::AsyncJob(handle), start_time)) => {
                emit(
                    subs,
                    RuntimeEvent::AsyncToolStarted {
                        tool: tool_call.name.clone(),
                        job_id: handle.job_id.clone(),
                    },
                )
                .await;

                let async_result = poll_async_job(
                    primary(subs),
                    &tool_call.name,
                    &handle,
                    start_time,
                    &state.webhook_runtime,
                )
                .await;

                let duration = start_time.elapsed();
                if async_result.get("error").is_some() {
                    telemetry::record_tool_error(&tool_call.name, source_label, duration);
                } else {
                    telemetry::record_tool_success(&tool_call.name, source_label, duration);
                }
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: async_result,
                });
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
            Err(e) => {
                if let Err(reason) = record_repeated_failure(state, subs, &tool_call.name, &e).await
                {
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: tool_error_result(&e),
                });
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
        }
    }

    apply_tool_phase_results(state, subs, run_id, pending_handoff, tool_results).await;

    true
}

async fn apply_tool_phase_results(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    run_id: RunId,
    pending_handoff: Option<PendingHandoff>,
    mut tool_results: Vec<ContentBlock>,
) {
    // Process pending handoff: prepare the next state first, then swap it in.
    let Some(pending_handoff) = pending_handoff else {
        state.messages.push(Message {
            role: Role::User,
            content: tool_results,
        });
        state.step += 1;
        return;
    };

    let PendingHandoff {
        tool_name,
        tool_use_id,
        result: handoff_result,
    } = pending_handoff;
    let previous_agent =
        state.config.system_prompt[..state.config.system_prompt.len().min(60)].to_string();
    let new_agent_prompt = handoff_result.target_agent.system_prompt.clone();
    let new_agent = new_agent_prompt[..new_agent_prompt.len().min(60)].to_string();
    let handoff_input = json!({"tool": tool_name});

    let mut handoff_history = state.messages.clone();
    handoff_history.push(Message {
        role: Role::User,
        content: tool_results.clone(),
    });

    let transition = prepare_handoff_transition(
        state,
        handoff_result,
        handoff_history,
        handoff_input.clone(),
    )
    .await;

    let transition = match transition {
        Ok(transition) => transition,
        Err(error) => {
            emit(
                subs,
                RuntimeEvent::ToolCallFailed {
                    tool: tool_name,
                    error: error.clone(),
                },
            )
            .await;
            replace_tool_result(&mut tool_results, &tool_use_id, tool_error_result(&error));
            state.messages.push(Message {
                role: Role::User,
                content: tool_results,
            });
            state.step += 1;
            return;
        }
    };

    crate::hook::runner::run_on_handoff(
        &state.config.hooks,
        &crate::hook::HandoffHookContext {
            run_id,
            previous_agent: previous_agent.clone(),
            new_agent: new_agent.clone(),
            handoff_input,
        },
        primary(subs),
    )
    .await;

    emit(
        subs,
        RuntimeEvent::AgentUpdated {
            previous_agent,
            new_agent: new_agent.clone(),
        },
    )
    .await;

    state.messages = transition.messages;
    state.registry = transition.registry;
    state.tool_defs = transition.tool_defs;
    state.budget = transition.budget;
    state.run_hook_ctx.agent_name = new_agent;
    state.config = transition.config;
    state.step += 1;
}

async fn prepare_handoff_transition(
    state: &AgentRunState,
    handoff_result: crate::handoff::HandoffResult,
    history: Vec<Message>,
    handoff_input: Value,
) -> Result<PreparedHandoffTransition, ToolError> {
    let new_agent_prompt = handoff_result.target_agent.system_prompt.clone();
    let mut next_messages = handoff_result
        .apply_filter(history, handoff_input)
        .await
        .map_err(|error| ToolError::fatal(error.to_string()).with_code("HANDOFF_FILTER_FAILED"))?;

    if next_messages
        .first()
        .map(|m| !matches!(m.role, Role::System))
        .unwrap_or(true)
    {
        next_messages.insert(
            0,
            Message {
                role: Role::System,
                content: vec![ContentBlock::Text(new_agent_prompt.clone())],
            },
        );
    } else if let Some(first) = next_messages.first_mut() {
        first.content = vec![ContentBlock::Text(new_agent_prompt)];
    }

    let mut new_config = handoff_result.target_agent;
    let mut next_registry = ToolRegistry::new();
    for handoff in new_config.handoffs.drain(..) {
        let tool: Arc<dyn Tool> = Arc::new(crate::tool::handoff_tool::HandoffTool::new(handoff));
        next_registry.register(tool).map_err(|error| {
            ToolError::fatal(format!("failed to build handoff registry: {error}"))
                .with_code("HANDOFF_REGISTRY_FAILED")
        })?;
    }
    let next_registry = next_registry.filter_by_allowed(&new_config.runtime.allowed_tools);
    next_registry.validate_metadata_links().map_err(|error| {
        ToolError::fatal(format!("tool metadata validation failed: {error}"))
            .with_code("TOOL_METADATA_VALIDATION_FAILED")
    })?;
    let tool_defs = next_registry.list();

    let new_budget = if new_config.budget.max_tokens.is_none()
        && new_config.budget.max_tool_calls.is_none()
        && new_config.budget.max_duration.is_none()
        && new_config.budget.max_cost_usd.is_none()
    {
        state.budget.remaining_config()
    } else {
        new_config.budget.clone()
    };
    let budget = BudgetGuard::new(new_budget);

    Ok(PreparedHandoffTransition {
        messages: next_messages,
        config: new_config,
        registry: next_registry,
        tool_defs,
        budget,
    })
}

fn replace_tool_result(tool_results: &mut [ContentBlock], tool_use_id: &str, content: Value) {
    for block in tool_results {
        if let ContentBlock::ToolResult {
            tool_use_id: existing,
            content: existing_content,
        } = block
        {
            if existing == tool_use_id {
                *existing_content = content;
                return;
            }
        }
    }
}

// ── Utility helpers ──────────────────────────────────────────────────────────

async fn emit(subs: &[mpsc::Sender<RuntimeEvent>], event: RuntimeEvent) {
    if let Some(primary) = subs.first() {
        match tokio::time::timeout(EVENT_SEND_TIMEOUT, primary.send(event.clone())).await {
            Ok(Ok(())) | Ok(Err(_)) => {}
            Err(_) => {
                telemetry::record_event_drop("primary", 1);
                if primary
                    .try_send(RuntimeEvent::EventsDropped {
                        subscriber_id: 0,
                        count: 1,
                    })
                    .is_err()
                {
                    tracing::warn!(
                        "primary event subscriber timed out and EventsDropped notification channel is full"
                    );
                }
            }
        }
    }
    for (subscriber_id, sub) in subs.iter().enumerate().skip(1) {
        if let Err(mpsc::error::TrySendError::Full(_)) = sub.try_send(event.clone()) {
            telemetry::record_event_drop("secondary", 1);
            if let Some(p) = subs.first() {
                if p.try_send(RuntimeEvent::EventsDropped {
                    subscriber_id: subscriber_id as u64,
                    count: 1,
                })
                .is_err()
                {
                    tracing::warn!(
                        subscriber_id,
                        "secondary event subscriber dropped an event and primary notification channel is full"
                    );
                }
            }
        }
    }
}

fn estimate_context_tokens(messages: &[Message], tool_defs: &[ToolDef]) -> u64 {
    let message_tokens = serde_json::to_string(messages)
        .map(|serialized| crate::tokenizer::count_tokens(&serialized) as u64)
        .unwrap_or(0);
    let tool_tokens = serde_json::to_string(tool_defs)
        .map(|serialized| (serialized.len() as u64).div_ceil(4))
        .unwrap_or(0);
    message_tokens + tool_tokens
}

fn primary(subs: &[mpsc::Sender<RuntimeEvent>]) -> &mpsc::Sender<RuntimeEvent> {
    subs.first()
        .expect("event_subs always has at least one subscriber")
}

fn approval_context_for(meta: &ToolMetadata) -> ApprovalContext {
    match &meta.execution_mode {
        ToolExecutionMode::Commit { draft_tool } => ApprovalContext::CommitToolCall {
            draft_tool: draft_tool.clone(),
        },
        ToolExecutionMode::Draft { .. } | ToolExecutionMode::Normal => {
            ApprovalContext::InitialToolCall
        }
    }
}

fn tool_source_label(source: &ToolSource) -> &'static str {
    match source {
        ToolSource::Builtin => "builtin",
        ToolSource::InProcess => "in_process",
        ToolSource::McpServer { .. } => "mcp_server",
        ToolSource::Skill { .. } => "skill",
    }
}

async fn run_parallel_tool_batch_if_allowed(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    run_id: RunId,
    tool_uses: &[ToolCall],
) -> Option<Vec<ContentBlock>> {
    if state.config.runtime.tool_execution_policy != ToolExecutionPolicy::ParallelSafe
        || tool_uses.len() < 2
        || !state.config.hooks.is_empty()
        || state.config.retry_policy.is_some()
    {
        return None;
    }
    if let Some(max) = state.config.budget.max_tool_calls {
        if state.budget.usage().tool_calls_used + tool_uses.len() as u32 > max {
            return None;
        }
    }

    let mut calls = Vec::with_capacity(tool_uses.len());
    for (idx, tool_call) in tool_uses.iter().enumerate() {
        if deferred_tool_exposure_error(state, &tool_call.name).is_some() {
            return None;
        }
        let tool = state.registry.get(&tool_call.name)?;
        let metadata = tool.metadata().clone();
        if metadata.side_effect
            || metadata.parallelism != ToolParallelism::ParallelSafe
            || state.config.runtime.should_approve(&metadata)
        {
            return None;
        }
        let source_label = tool_source_label(&metadata.source);
        calls.push(ParallelToolCall {
            requested_order: idx,
            tool_call: tool_call.clone(),
            tool,
            metadata: metadata.clone(),
            input: tool_call.input.clone(),
            source_label,
            max_output_tokens: metadata.max_output_tokens,
        });
    }

    let batch_id = format!("tool_batch_{}", uuid::Uuid::new_v4());
    emit(
        subs,
        RuntimeEvent::ToolCallBatchStarted {
            batch_id: batch_id.clone(),
            tool_count: calls.len(),
        },
    )
    .await;
    for call in &calls {
        state.budget.record_tool_call();
        emit(
            subs,
            RuntimeEvent::ToolCallBatchItemStarted {
                batch_id: batch_id.clone(),
                tool: call.tool_call.name.clone(),
                requested_order: call.requested_order,
            },
        )
        .await;
    }

    let mut pending = FuturesUnordered::new();
    for call in calls {
        let event_tx = primary(subs).clone();
        let webhook_base_url = state.webhook_runtime.as_ref().map(|rt| rt.base_url.clone());
        let approval_bus = state.approval_bus.clone();
        let remaining_budget = state.budget.remaining_config();
        let parent_messages = if call.tool.needs_parent_context() {
            state.messages.clone()
        } else {
            vec![]
        };
        pending.push(execute_parallel_tool_call(
            call,
            ParallelExecutionContext {
                run_id,
                run_depth: state.config.runtime.run_depth,
                event_tx,
                webhook_base_url,
                approval_bus,
                remaining_budget,
                parent_messages,
            },
        ));
    }

    let mut ordered_results: Vec<Option<ContentBlock>> = vec![None; tool_uses.len()];
    let mut completion_order = 0;
    while let Some(result) = pending.next().await {
        completion_order += 1;
        let requested_order = result.requested_order;
        let tool_name = result.tool_call.name.clone();
        emit(
            subs,
            RuntimeEvent::ToolCallBatchItemCompleted {
                batch_id: batch_id.clone(),
                tool: tool_name.clone(),
                requested_order,
                completion_order,
            },
        )
        .await;
        let content = finalize_parallel_tool_result(state, subs, result).await;
        ordered_results[requested_order] = Some(content);
    }

    Some(
        ordered_results
            .into_iter()
            .flatten()
            .collect::<Vec<ContentBlock>>(),
    )
}

async fn execute_parallel_tool_call(
    call: ParallelToolCall,
    parallel_ctx: ParallelExecutionContext,
) -> ParallelToolResult {
    let ctx = ToolContext {
        run_id: parallel_ctx.run_id,
        run_depth: parallel_ctx.run_depth,
        tool_call_id: call.tool_call.id.clone(),
        event_tx: Some(parallel_ctx.event_tx.clone()),
        webhook_base_url: parallel_ctx.webhook_base_url,
        approval_bus: parallel_ctx.approval_bus,
        remaining_budget: parallel_ctx.remaining_budget,
        parent_messages: parallel_ctx.parent_messages,
    };
    let _ = parallel_ctx
        .event_tx
        .send(RuntimeEvent::ToolCallStarted {
            tool: call.tool_call.name.clone(),
            metadata: call.metadata.clone(),
            input: call.input.clone(),
        })
        .await;
    let start_time = Instant::now();
    let execute_fut = call.tool.execute(call.input.clone(), &ctx);
    let result = if let Some(timeout) = call.metadata.timeout {
        match tokio::time::timeout(timeout, execute_fut).await {
            Ok(result) => result.map(|output| (output, start_time)),
            Err(_) => Err(ToolError::transient("tool execution timed out").with_code("TIMEOUT")),
        }
    } else {
        execute_fut.await.map(|output| (output, start_time))
    };

    ParallelToolResult {
        requested_order: call.requested_order,
        tool_call: call.tool_call,
        source_label: call.source_label,
        max_output_tokens: call.max_output_tokens,
        result,
    }
}

async fn finalize_parallel_tool_result(
    state: &mut AgentRunState,
    subs: &[mpsc::Sender<RuntimeEvent>],
    result: ParallelToolResult,
) -> ContentBlock {
    match result.result {
        Ok((ToolOutput::Immediate(value), start_time)) => {
            let mut value = value;
            if let Some(max_tokens) = result.max_output_tokens {
                value = truncate_output(value, max_tokens);
            }
            let duration = start_time.elapsed();
            emit(
                subs,
                RuntimeEvent::ToolCallCompleted {
                    tool: result.tool_call.name.clone(),
                    output: value.clone(),
                    duration,
                },
            )
            .await;
            telemetry::record_tool_success(&result.tool_call.name, result.source_label, duration);
            ContentBlock::ToolResult {
                tool_use_id: result.tool_call.id,
                content: value,
            }
        }
        Ok((
            ToolOutput::Structured {
                model_output,
                details,
                external_usage,
            },
            start_time,
        )) => {
            if let Some(usage) = external_usage {
                state.budget.record_external_usage(&usage);
            }
            let mut model_output = model_output;
            let mut details = details;
            if let Some(max_tokens) = result.max_output_tokens {
                model_output = truncate_output(model_output, max_tokens);
                details = truncate_output(details, max_tokens);
            }
            let duration = start_time.elapsed();
            emit(
                subs,
                RuntimeEvent::ToolCallCompleted {
                    tool: result.tool_call.name.clone(),
                    output: details,
                    duration,
                },
            )
            .await;
            telemetry::record_tool_success(&result.tool_call.name, result.source_label, duration);
            ContentBlock::ToolResult {
                tool_use_id: result.tool_call.id,
                content: model_output,
            }
        }
        Ok((ToolOutput::AsyncJob(handle), start_time)) => {
            emit(
                subs,
                RuntimeEvent::AsyncToolStarted {
                    tool: result.tool_call.name.clone(),
                    job_id: handle.job_id.clone(),
                },
            )
            .await;
            let async_result = poll_async_job(
                primary(subs),
                &result.tool_call.name,
                &handle,
                start_time,
                &state.webhook_runtime,
            )
            .await;
            let duration = start_time.elapsed();
            if async_result.get("error").is_some() {
                telemetry::record_tool_error(&result.tool_call.name, result.source_label, duration);
            } else {
                telemetry::record_tool_success(
                    &result.tool_call.name,
                    result.source_label,
                    duration,
                );
            }
            ContentBlock::ToolResult {
                tool_use_id: result.tool_call.id,
                content: async_result,
            }
        }
        Ok((ToolOutput::Handoff(_), _)) => {
            let error = ToolError::fatal("handoff tools cannot execute in parallel")
                .with_code("PARALLEL_HANDOFF");
            ContentBlock::ToolResult {
                tool_use_id: result.tool_call.id,
                content: tool_error_result(&error),
            }
        }
        Err(error) => {
            emit(
                subs,
                RuntimeEvent::ToolCallFailed {
                    tool: result.tool_call.name.clone(),
                    error: error.clone(),
                },
            )
            .await;
            telemetry::record_tool_error(
                &result.tool_call.name,
                result.source_label,
                Duration::default(),
            );
            ContentBlock::ToolResult {
                tool_use_id: result.tool_call.id,
                content: tool_error_result(&error),
            }
        }
    }
}

fn deferred_tool_exposure_error(state: &AgentRunState, tool_name: &str) -> Option<ToolError> {
    if !state.config.runtime.tool_search_enabled || tool_name == "search_tools" {
        return None;
    }
    if state.tool_defs.iter().any(|tool| tool.name == tool_name) {
        return None;
    }
    state.registry.contains(tool_name).then(|| {
        ToolError::fatal(format!(
            "tool '{tool_name}' is hidden until returned by search_tools"
        ))
        .with_code("NOT_EXPOSED")
        .with_next_step("call search_tools and retry after the tool schema is exposed")
    })
}

#[allow(clippy::too_many_arguments)] // justified: mirrors pre_start owned state needed to terminate startup cleanly
async fn fail_pre_start(
    myself: &ActorRef<AgentMsg>,
    event_subs: &[mpsc::Sender<RuntimeEvent>],
    run_id: RunId,
    config: AgentConfig,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    approval_bus: ApprovalBus,
    run_hook_ctx: crate::hook::RunHookContext,
    error: String,
) -> AgentRunState {
    emit(event_subs, RuntimeEvent::RunFailed { error }).await;
    myself.cast(AgentMsg::RunStep).ok();
    failed_state(
        run_id,
        config,
        model,
        registry,
        approval_bus,
        event_subs.to_vec(),
        run_hook_ctx,
    )
}

#[allow(clippy::too_many_arguments)] // justified: mirrors AgentRunArgs fields for error recovery path
fn failed_state(
    run_id: RunId,
    config: AgentConfig,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    approval_bus: ApprovalBus,
    event_subs: Vec<mpsc::Sender<RuntimeEvent>>,
    run_hook_ctx: crate::hook::RunHookContext,
) -> AgentRunState {
    AgentRunState {
        run_id,
        config,
        model,
        unfiltered_registry: registry.clone(),
        registry,
        messages: vec![],
        tool_defs: vec![],
        step: 0,
        budget: BudgetGuard::new(BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        }),
        last_compaction_step: None,
        cancelled: true,
        run_hook_ctx,
        approval_bus,
        event_subs,
        webhook_runtime: None,
        repeated_failures: HashMap::new(),
    }
}

#[cfg(test)]
mod history_clone_profile_tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ClonePath {
        ModelCall { retry_count: usize },
        Handoff { input_filter: bool },
    }

    #[derive(Debug, PartialEq, Eq)]
    struct MessageHistoryCloneProfile {
        path: ClonePath,
        message_count: usize,
        content_bytes: usize,
        clone_count: usize,
        estimated_payload_bytes: usize,
    }

    fn profile_message_history_clones(
        messages: &[Message],
        path: ClonePath,
    ) -> MessageHistoryCloneProfile {
        let clone_count = match path {
            // One pre-step snapshot plus one hook/model-call clone per attempt.
            ClonePath::ModelCall { retry_count } => 1 + retry_count + 1,
            // The handoff phase clones state.messages. Input filters currently
            // receive an owned HandoffInputData and add one defensive clone.
            ClonePath::Handoff { input_filter } => 1 + usize::from(input_filter),
        };
        let content_bytes = message_content_bytes(messages);
        MessageHistoryCloneProfile {
            path,
            message_count: messages.len(),
            content_bytes,
            clone_count,
            estimated_payload_bytes: content_bytes * clone_count,
        }
    }

    fn message_content_bytes(messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|message| {
                message
                    .content
                    .iter()
                    .map(content_block_payload_bytes)
                    .sum::<usize>()
            })
            .sum()
    }

    fn content_block_payload_bytes(block: &ContentBlock) -> usize {
        match block {
            ContentBlock::Text(text) => text.len(),
            ContentBlock::Thinking {
                text,
                signature,
                provider_details,
            } => {
                text.as_deref().map(str::len).unwrap_or(0)
                    + signature.as_deref().map(str::len).unwrap_or(0)
                    + provider_details
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()
                        .unwrap_or_default()
                        .map(|value| value.len())
                        .unwrap_or(0)
            }
            ContentBlock::ToolUse { id, name, input } => {
                id.len() + name.len() + serde_json::to_string(input).unwrap_or_default().len()
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
            } => tool_use_id.len() + serde_json::to_string(content).unwrap_or_default().len(),
            // v0.9.10 multimodal variants — payload is the encoded source +
            // (for Video) a few small numeric fields. Approximate via the
            // source size; tests using this helper only care about relative
            // magnitudes for compaction triggering, not exact bytes.
            ContentBlock::Image { source, detail } => {
                media_source_bytes(source) + detail.as_deref().map(str::len).unwrap_or(0)
            }
            ContentBlock::Video {
                source,
                fps: _,
                detail,
                max_long_side_pixel: _,
            } => media_source_bytes(source) + detail.as_deref().map(str::len).unwrap_or(0),
            ContentBlock::Audio { source } => media_source_bytes(source),
            ContentBlock::MidConvSystem(text) => text.len(),
        }
    }

    fn media_source_bytes(source: &orchest_protocol::MediaSource) -> usize {
        match source {
            orchest_protocol::MediaSource::Url { url } => url.len(),
            orchest_protocol::MediaSource::Base64 { media_type, data } => {
                media_type.len() + data.len()
            }
        }
    }

    fn representative_history(message_count: usize, payload_bytes: usize) -> Vec<Message> {
        (0..message_count)
            .map(|idx| Message {
                role: if idx == 0 { Role::System } else { Role::User },
                content: vec![ContentBlock::Text("x".repeat(payload_bytes))],
            })
            .collect()
    }

    #[test]
    fn message_history_clone_profile_records_model_retry_and_handoff_paths() {
        let history = representative_history(64, 1024);

        let model_call =
            profile_message_history_clones(&history, ClonePath::ModelCall { retry_count: 0 });
        let one_retry =
            profile_message_history_clones(&history, ClonePath::ModelCall { retry_count: 1 });
        let handoff = profile_message_history_clones(
            &history,
            ClonePath::Handoff {
                input_filter: false,
            },
        );
        let handoff_filter =
            profile_message_history_clones(&history, ClonePath::Handoff { input_filter: true });

        eprintln!("message history clone profile:");
        for profile in [&model_call, &one_retry, &handoff, &handoff_filter] {
            eprintln!(
                "path={:?} messages={} content_bytes={} clone_count={} estimated_payload_bytes={}",
                profile.path,
                profile.message_count,
                profile.content_bytes,
                profile.clone_count,
                profile.estimated_payload_bytes
            );
        }

        assert_eq!(model_call.message_count, 64);
        assert_eq!(model_call.content_bytes, 64 * 1024);
        assert_eq!(model_call.clone_count, 2);
        assert_eq!(one_retry.clone_count, 3);
        assert_eq!(handoff.clone_count, 1);
        assert_eq!(handoff_filter.clone_count, 2);
        assert_eq!(one_retry.estimated_payload_bytes, 64 * 1024 * 3);
    }
}
