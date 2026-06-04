//! WorkerActor: Ractor-based agent run actor (mode B: self-message step).

use std::sync::Arc;
use std::time::{Duration, Instant};

use ractor::{Actor, ActorProcessingErr, ActorRef};
use serde_json::json;
use tokio::sync::mpsc;

use crate::budget::{BudgetConfig, BudgetGuard};
use crate::events::RuntimeEvent;
use crate::model::{ContentBlock, Message, ModelAdapter, ModelStreamChunk, Role, StopReason};
use crate::telemetry;
use crate::tool::code_exec::CodeExecutionMcpServer;
use crate::tool::registry::ToolRegistry;
use crate::tool::search::SearchToolsTool;
use crate::tool::{Tool, ToolCall, ToolContext, ToolDef, ToolError, ToolOutput};

use super::compaction::maybe_compact_context;
use super::config::{AgentConfig, RunId};
use super::handle::ApprovalBus;
use super::helpers::{append_searched_tool_defs, connect_mcp_servers, truncate_output};
use super::skills::register_skills;
use super::tool_exec::poll_async_job;
use super::webhook::{start_webhook_server, WebhookRuntime};

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(3600);

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
pub(crate) struct ResumeState {
    pub messages: Vec<crate::model::Message>,
    pub step: u32,
    pub budget_used: crate::budget::BudgetUsage,
}

pub(crate) struct AgentRunArgs {
    pub run_id: RunId,
    pub config: AgentConfig,
    pub input: String,
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
            emit(
                &event_subs,
                RuntimeEvent::RunFailed {
                    error: error.message,
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

        if config.runtime.code_execution_enabled {
            for tool in CodeExecutionMcpServer::tools() {
                if let Err(error) = registry.register(tool) {
                    emit(
                        &event_subs,
                        RuntimeEvent::RuntimeWarning {
                            message: format!("failed to register code execution tool: {error}"),
                        },
                    )
                    .await;
                }
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
                emit(
                    &event_subs,
                    RuntimeEvent::RunFailed {
                        error: format!("skill loading failed: {error}"),
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
                content: vec![ContentBlock::Text(input)],
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

/// Execute one outer-loop iteration. Returns true to continue, false to stop.
#[allow(clippy::too_many_lines)] // justified: single-function orchestration loop; splitting would obscure control flow
async fn run_one_step(state: &mut AgentRunState) -> bool {
    let subs = state.event_subs.clone();
    let step = state.step;
    let run_id = state.run_id;

    if step >= state.config.runtime.max_steps {
        state.refresh_terminal_hook_ctx(step);
        crate::hook::runner::run_on_run_error(
            &state.config.hooks,
            &state.run_hook_ctx,
            "max_steps_reached",
            primary(&subs),
        )
        .await;
        emit(
            &subs,
            RuntimeEvent::RunFailed {
                error: "max_steps_reached".into(),
            },
        )
        .await;
        return false;
    }

    if let Some(violation) = state.budget.check() {
        emit(
            &subs,
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
            primary(&subs),
        )
        .await;
        emit(&subs, RuntimeEvent::RunFailed { error }).await;
        return false;
    }

    emit(&subs, RuntimeEvent::ModelCallStarted { step }).await;

    // Snapshot the message history before this step's hooks.
    // Each before_model invocation (including retries) receives a fresh clone
    // of this snapshot, so hook injections are ephemeral: they are passed to
    // the model for this call but never accumulate in the durable conversation
    // history stored in state.messages.
    let pre_step_messages = state.messages.clone();
    let mut retry_attempt: u32 = 0;
    let response = loop {
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
                primary(&subs),
            )
            .await
            {
                crate::hook::ModelHookAction::Abort(reason) => {
                    state.refresh_terminal_hook_ctx(step);
                    crate::hook::runner::run_on_run_error(
                        &state.config.hooks,
                        &state.run_hook_ctx,
                        &reason,
                        primary(&subs),
                    )
                    .await;
                    emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
                crate::hook::ModelHookAction::Continue => {}
            }
            model_ctx.messages
        };

        let (stream_tx, mut stream_rx) = mpsc::channel::<ModelStreamChunk>(64);
        let event_tx_clone = primary(&subs).clone();
        let forward_task = tokio::spawn(async move {
            while let Some(chunk) = stream_rx.recv().await {
                let _ = event_tx_clone
                    .send(RuntimeEvent::ModelStreamChunk { delta: chunk })
                    .await;
            }
        });

        let raw_response = state
            .model
            .complete(
                &call_messages,
                &state.tool_defs,
                &state.config.model.options,
                Some(stream_tx),
            )
            .await;
        let _ = forward_task.await;

        match raw_response {
            Ok(r) => {
                let mut r = r;
                {
                    // after_model sees the call-time context that was sent to the model,
                    // plus the model's response (which the hook may rewrite).
                    let mut model_ctx = crate::hook::ModelHookContext {
                        run_id,
                        messages: call_messages,
                        model_spec: state.config.model.spec.clone(),
                        response: Some(r.content.clone()),
                    };
                    if let crate::hook::HookAction::Abort(reason) =
                        crate::hook::runner::run_after_model(
                            &state.config.hooks,
                            &mut model_ctx,
                            primary(&subs),
                        )
                        .await
                    {
                        state.refresh_terminal_hook_ctx(step);
                        crate::hook::runner::run_on_run_error(
                            &state.config.hooks,
                            &state.run_hook_ctx,
                            &reason,
                            primary(&subs),
                        )
                        .await;
                        emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                        return false;
                    }
                    // Read back any rewrite the hook applied to the response.
                    if let Some(modified) = model_ctx.response.take() {
                        r.content = modified;
                    }
                }
                break r;
            }
            Err(e) => {
                let class = super::retry::classify(&e);
                if super::retry::should_retry(&class, retry_attempt, &state.config.retry_policy) {
                    let delay = super::retry::compute_delay(
                        retry_attempt,
                        &e,
                        state.config.retry_policy.as_ref().unwrap(),
                    );
                    emit(
                        &subs,
                        RuntimeEvent::ModelRetry {
                            attempt: retry_attempt + 1,
                            error: e.message.clone(),
                            next_delay: delay,
                        },
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    retry_attempt += 1;
                    continue;
                }
                let error = e.to_string();
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
        }
    };

    state.budget.record_model_call(&response.usage);

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

    match response.stop_reason {
        StopReason::EndTurn if tool_uses.is_empty() => {
            let output = json!(text_parts.join(""));
            state.refresh_terminal_hook_ctx(step);
            crate::hook::runner::run_on_run_end(
                &state.config.hooks,
                &state.run_hook_ctx,
                primary(&subs),
            )
            .await;
            emit(&subs, RuntimeEvent::RunCompleted { output }).await;
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
            emit(&subs, RuntimeEvent::RunCompleted { output }).await;
            return false;
        }
        _ => {}
    }

    state.messages.push(Message {
        role: Role::Assistant,
        content: response.content.clone(),
    });

    let mut tool_results = Vec::new();
    let mut handoff_triggered = false;
    let mut pending_handoff: Option<(String, crate::handoff::HandoffResult)> = None;

    for tool_call in &tool_uses {
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
                    &subs,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_call.name.clone(),
                        error: error.clone(),
                    },
                )
                .await;
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!({"error": error.message}),
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
            primary(&subs),
        )
        .await
        {
            crate::hook::HookAction::Skip => {
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!("tool call skipped by hook"),
                });
                state.budget.record_tool_call();
                continue;
            }
            crate::hook::HookAction::Reject(reason) => {
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!({"error": reason}),
                });
                state.budget.record_tool_call();
                continue;
            }
            crate::hook::HookAction::Abort(reason) => {
                emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
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
            let approval_rx = state.approval_bus.request(run_id).await;
            emit(
                &subs,
                RuntimeEvent::ApprovalRequested {
                    tool_call: effective_call.clone(),
                },
            )
            .await;

            let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, approval_rx).await {
                Ok(result) => result.unwrap_or(false),
                Err(_) => {
                    emit(
                        &subs,
                        RuntimeEvent::RunFailed {
                            error: "approval_timeout".into(),
                        },
                    )
                    .await;
                    return false;
                }
            };
            state.approval_bus.cancel(run_id).await;

            if approved {
                emit(
                    &subs,
                    RuntimeEvent::ApprovalGranted {
                        tool_call: effective_call.clone(),
                    },
                )
                .await;
            } else {
                emit(
                    &subs,
                    RuntimeEvent::ApprovalDenied {
                        tool_call: effective_call.clone(),
                    },
                )
                .await;
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!({"error": "tool call denied by user"}),
                });
                continue;
            }
        }

        if let Some(max) = state.config.budget.max_tool_calls {
            if state.budget.usage().tool_calls_used >= max {
                emit(
                    &subs,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_call.name.clone(),
                        error: ToolError::fatal("tool call budget exceeded")
                            .with_code("BUDGET_EXCEEDED"),
                    },
                )
                .await;
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!({"error": "tool call budget exceeded"}),
                });
                continue;
            }
        }

        // ToolCallStarted fires only once we are committed to executing the tool
        // (after before_tool, approval, and budget checks all pass).
        emit(
            &subs,
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
            on_update: None,
            event_tx: Some(primary(&subs).clone()),
            webhook_base_url: state.webhook_runtime.as_ref().map(|rt| rt.base_url.clone()),
            approval_bus: state.approval_bus.clone(),
            remaining_budget: state.budget.remaining_config(),
            parent_messages,
        };

        let start_time = Instant::now();
        let metadata_timeout = tool_meta.timeout;
        let max_output_tokens = tool_meta.max_output_tokens;
        let execute_fut = tool.execute(tool_input.clone(), &ctx);
        let result = if let Some(timeout) = metadata_timeout {
            match tokio::time::timeout(timeout, execute_fut).await {
                Ok(r) => r,
                Err(_) => {
                    telemetry::record_tool_timeout(
                        &tool_call.name,
                        source_label,
                        start_time.elapsed(),
                    );
                    emit(
                        &subs,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: ToolError::transient("tool execution timed out")
                                .with_code("TIMEOUT"),
                        },
                    )
                    .await;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": "tool execution timed out"}),
                    });
                    state.budget.record_tool_call();
                    continue;
                }
            }
        } else {
            execute_fut.await
        };

        match result {
            Ok(ToolOutput::Immediate(value)) => {
                let mut value = value;
                if let Some(max_tokens) = max_output_tokens {
                    value = truncate_output(value, max_tokens);
                }
                if state.config.runtime.tool_search_enabled && tool_call.name == "search_tools" {
                    append_searched_tool_defs(&mut state.tool_defs, &value);
                }
                let duration = start_time.elapsed();
                emit(
                    &subs,
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
                    &subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
            Ok(ToolOutput::Structured {
                model_output,
                details,
                external_usage,
            }) => {
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
                    &subs,
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
                    &subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
            Ok(ToolOutput::Handoff(result)) => {
                state.budget.record_tool_call();
                if handoff_triggered {
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": "Only one handoff per turn is allowed"}),
                    });
                } else {
                    handoff_triggered = true;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"result": result.transfer_message}),
                    });
                    pending_handoff = Some((tool_call.name.clone(), *result));
                }
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    &subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
                continue;
            }
            Ok(ToolOutput::AsyncJob(handle)) => {
                emit(
                    &subs,
                    RuntimeEvent::AsyncToolStarted {
                        tool: tool_call.name.clone(),
                        job_id: handle.job_id.clone(),
                    },
                )
                .await;

                let async_result = poll_async_job(
                    primary(&subs),
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
                    &subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
            Err(e) => {
                let duration = start_time.elapsed();
                emit(
                    &subs,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_call.name.clone(),
                        error: e.clone(),
                    },
                )
                .await;
                telemetry::record_tool_error(&tool_call.name, source_label, duration);
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!({"error": e.message}),
                });
                if let Err(reason) = finalize_after_tool(
                    &state.config.hooks,
                    &subs,
                    run_id,
                    &tool_call.name,
                    &tool_input,
                    &tool_meta,
                    &mut tool_results,
                )
                .await
                {
                    emit(&subs, RuntimeEvent::RunFailed { error: reason }).await;
                    return false;
                }
            }
        }

        state.budget.record_tool_call();
    }

    state.messages.push(Message {
        role: Role::User,
        content: tool_results,
    });

    // Process pending handoff: switch agent and continue
    if let Some((tool_name, handoff_result)) = pending_handoff {
        let previous_agent =
            state.config.system_prompt[..state.config.system_prompt.len().min(60)].to_string();
        let new_agent_prompt = handoff_result.target_agent.system_prompt.clone();
        let new_agent = new_agent_prompt[..new_agent_prompt.len().min(60)].to_string();

        crate::hook::runner::run_on_handoff(
            &state.config.hooks,
            &crate::hook::HandoffHookContext {
                run_id,
                previous_agent: previous_agent.clone(),
                new_agent: new_agent.clone(),
                handoff_input: json!({"tool": tool_name}),
            },
            primary(&subs),
        )
        .await;

        emit(
            &subs,
            RuntimeEvent::AgentUpdated {
                previous_agent,
                new_agent: new_agent.clone(),
            },
        )
        .await;

        let mut next_messages = handoff_result
            .apply_filter(state.messages.clone(), json!({"tool": tool_name}))
            .await;

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
            first.content = vec![ContentBlock::Text(new_agent_prompt.clone())];
        }
        state.messages = next_messages;

        state.registry = ToolRegistry::new();
        let mut new_config = handoff_result.target_agent;
        for handoff in new_config.handoffs.drain(..) {
            let tool: Arc<dyn Tool> =
                Arc::new(crate::tool::handoff_tool::HandoffTool::new(handoff));
            let _ = state.registry.register(tool);
        }
        state.registry = state
            .registry
            .filter_by_allowed(&new_config.runtime.allowed_tools);
        state.tool_defs = state.registry.list();

        let new_budget = if new_config.budget.max_tokens.is_none()
            && new_config.budget.max_tool_calls.is_none()
            && new_config.budget.max_duration.is_none()
            && new_config.budget.max_cost_usd.is_none()
        {
            state.budget.remaining_config()
        } else {
            new_config.budget.clone()
        };
        state.budget = BudgetGuard::new(new_budget);

        state.run_hook_ctx.agent_name = new_agent;
        state.config = new_config;
    }

    state.step += 1;
    true
}

// ── Utility helpers ──────────────────────────────────────────────────────────

async fn emit(subs: &[mpsc::Sender<RuntimeEvent>], event: RuntimeEvent) {
    // Primary subscriber: blocking send (preserves existing behaviour).
    if let Some(primary) = subs.first() {
        let _ = primary.send(event.clone()).await;
    }
    // Additional subscribers: try_send with backpressure.
    for (subscriber_id, sub) in subs.iter().enumerate().skip(1) {
        if let Err(mpsc::error::TrySendError::Full(_)) = sub.try_send(event.clone()) {
            if let Some(p) = subs.first() {
                let _ = p.try_send(RuntimeEvent::EventsDropped {
                    subscriber_id: subscriber_id as u64,
                    count: 1,
                });
            }
        }
    }
}

fn primary(subs: &[mpsc::Sender<RuntimeEvent>]) -> &mpsc::Sender<RuntimeEvent> {
    subs.first()
        .expect("event_subs always has at least one subscriber")
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
    }
}
