//! Main agent run loop: step execution, tool dispatch, and termination.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::sync::mpsc;

use crate::budget::BudgetGuard;
use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelResponse, ModelStreamChunk, Role, StopReason,
};
use crate::telemetry;
use crate::tool::code_exec::CodeExecutionMcpServer;
use crate::tool::registry::ToolRegistry;
use crate::tool::search::SearchToolsTool;
use crate::tool::{Tool, ToolCall, ToolContext, ToolDef, ToolOutput};

use super::compaction::maybe_compact_context;
use super::config::{AgentConfig, RunId};
use super::handle::ApprovalBus;
use super::helpers::{append_searched_tool_defs, connect_mcp_servers, emit, truncate_output};
use super::skills::register_skills;
use super::sub_agent::execute_agent_delegate;
use super::tool_exec::poll_async_job;
use super::webhook::start_webhook_server;
use tokio_util::sync::CancellationToken;

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(3600);

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_loop(
    run_id: RunId,
    config: AgentConfig,
    input: String,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    tx: mpsc::Sender<RuntimeEvent>,
    approval_bus: ApprovalBus,
    cancel_token: CancellationToken,
) {
    run_loop_inner(
        run_id,
        config,
        input,
        model,
        registry,
        tx,
        approval_bus.clone(),
        cancel_token,
    )
    .await;
    approval_bus.cancel(run_id).await;
}

#[allow(clippy::too_many_arguments)]
async fn run_loop_inner(
    run_id: RunId,
    config: AgentConfig,
    input: String,
    model: Arc<dyn ModelAdapter>,
    mut registry: ToolRegistry,
    tx: mpsc::Sender<RuntimeEvent>,
    approval_bus: ApprovalBus,
    cancel_token: CancellationToken,
) {
    emit(&tx, RuntimeEvent::RunStarted { run_id }).await;

    let webhook_runtime = if config.runtime.webhook_enabled {
        match start_webhook_server().await {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                emit(&tx, RuntimeEvent::RuntimeWarning { message: error }).await;
                None
            }
        }
    } else {
        None
    };

    if let Err(error) = connect_mcp_servers(&config, &mut registry).await {
        emit(
            &tx,
            RuntimeEvent::RunFailed {
                error: error.message,
            },
        )
        .await;
        return;
    }

    if config.runtime.code_execution_enabled {
        for tool in CodeExecutionMcpServer::tools() {
            if let Err(error) = registry.register(tool) {
                emit(
                    &tx,
                    RuntimeEvent::RuntimeWarning {
                        message: format!("failed to register code execution tool: {error}"),
                    },
                )
                .await;
            }
        }
    }

    // Register skill bundled tools when skills_dir is provided
    if let Some(ref skills_dir) = config.skills.dir {
        if let Err(error) =
            register_skills(skills_dir, &config.skills.allowed, &mut registry, &tx).await
        {
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: format!("skill loading failed: {error}"),
                },
            )
            .await;
            return;
        }
    }

    // Enforce allowed_tools: filter registry so only permitted tools are visible and executable
    let unfiltered_registry = registry.clone();
    let mut registry = registry.filter_by_allowed(&config.runtime.allowed_tools);

    let mut messages = vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text(config.system_prompt.clone())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text(input)],
        },
    ];

    let all_tool_defs = registry.list();
    let mut tool_defs = if config.runtime.tool_search_enabled {
        let search_tool = Arc::new(SearchToolsTool::new(all_tool_defs));
        let search_def = ToolDef {
            name: search_tool.name().to_string(),
            description: search_tool.description().to_string(),
            input_schema: search_tool.input_schema().clone(),
        };
        if let Err(error) = registry.register(search_tool) {
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: error.to_string(),
                },
            )
            .await;
            return;
        }
        vec![search_def]
    } else {
        all_tool_defs
    };
    let mut step: u32 = 0;
    let mut budget = BudgetGuard::new(config.budget.clone());
    let mut last_compaction_step: Option<u32> = None;

    loop {
        if cancel_token.is_cancelled() {
            emit(&tx, RuntimeEvent::RunAborted).await;
            return;
        }

        if step >= config.runtime.max_steps {
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: "max_steps_reached".into(),
                },
            )
            .await;
            return;
        }

        if let Some(violation) = budget.check() {
            emit(
                &tx,
                RuntimeEvent::BudgetWarning {
                    used: budget.usage().clone(),
                    limit: budget.config().clone(),
                },
            )
            .await;
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: format!("budget_exceeded: {violation}"),
                },
            )
            .await;
            return;
        }

        emit(&tx, RuntimeEvent::ModelCallStarted { step }).await;

        let (stream_tx, mut stream_rx) = mpsc::channel::<ModelStreamChunk>(64);
        let event_tx_clone = tx.clone();
        let forward_task = tokio::spawn(async move {
            while let Some(chunk) = stream_rx.recv().await {
                let _ = event_tx_clone
                    .send(RuntimeEvent::ModelStreamChunk { delta: chunk })
                    .await;
            }
        });

        let response = model
            .complete(
                &messages,
                &tool_defs,
                &config.model.options,
                Some(stream_tx),
            )
            .await;
        let _ = forward_task.await;

        let response: ModelResponse = match response {
            Ok(r) => r,
            Err(e) => {
                emit(
                    &tx,
                    RuntimeEvent::RunFailed {
                        error: e.to_string(),
                    },
                )
                .await;
                return;
            }
        };

        budget.record_model_call(&response.usage);

        emit(
            &tx,
            RuntimeEvent::ModelCallCompleted {
                tokens: response.usage.clone(),
                option_adjustments: response.option_adjustments.clone(),
            },
        )
        .await;

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last_compaction_step,
            step,
            &response.usage,
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
                emit(&tx, RuntimeEvent::RunCompleted { output }).await;
                return;
            }
            StopReason::MaxTokens if tool_uses.is_empty() => {
                let output = json!(text_parts.join(""));
                emit(&tx, RuntimeEvent::RunCompleted { output }).await;
                return;
            }
            _ => {}
        }

        messages.push(Message {
            role: Role::Assistant,
            content: response.content.clone(),
        });

        let mut tool_results = Vec::new();

        for tool_call in &tool_uses {
            let tool = match registry.get(&tool_call.name) {
                Some(t) => t,
                None => {
                    let error = if unfiltered_registry.contains(&tool_call.name) {
                        "tool not allowed".to_string()
                    } else {
                        format!("tool '{}' not found", tool_call.name)
                    };
                    emit(
                        &tx,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: error.clone(),
                        },
                    )
                    .await;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": error}),
                    });
                    continue;
                }
            };

            if tool.metadata().requires_approval {
                let approval_rx = approval_bus.request(run_id).await;
                emit(
                    &tx,
                    RuntimeEvent::ApprovalRequested {
                        tool_call: tool_call.clone(),
                    },
                )
                .await;

                let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, approval_rx).await {
                    Ok(result) => result.unwrap_or(false),
                    Err(_) => {
                        emit(
                            &tx,
                            RuntimeEvent::RunFailed {
                                error: "approval_timeout".into(),
                            },
                        )
                        .await;
                        return;
                    }
                };
                approval_bus.cancel(run_id).await;

                if approved {
                    emit(
                        &tx,
                        RuntimeEvent::ApprovalGranted {
                            tool_call: tool_call.clone(),
                        },
                    )
                    .await;
                } else {
                    emit(
                        &tx,
                        RuntimeEvent::ApprovalDenied {
                            tool_call: tool_call.clone(),
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

            if let Some(max) = config.budget.max_tool_calls {
                if budget.usage().tool_calls_used >= max {
                    emit(
                        &tx,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: "tool call budget exceeded".into(),
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

            let source = tool.metadata().source.clone();
            let source_label = match &source {
                crate::tool::ToolSource::Builtin => "builtin",
                crate::tool::ToolSource::InProcess => "in_process",
                crate::tool::ToolSource::McpServer { .. } => "mcp_server",
                crate::tool::ToolSource::Skill { .. } => "skill",
            };
            emit(
                &tx,
                RuntimeEvent::ToolCallStarted {
                    tool: tool_call.name.clone(),
                    source,
                    input: tool_call.input.clone(),
                },
            )
            .await;

            let _tool_span = telemetry::tool_execute_span(&tool_call.name, source_label);

            let ctx = ToolContext {
                run_id,
                run_depth: config.runtime.run_depth,
                tool_call_id: tool_call.id.clone(),
                on_update: None,
                event_tx: Some(tx.clone()),
                webhook_base_url: webhook_runtime
                    .as_ref()
                    .map(|runtime| runtime.base_url.clone()),
            };

            let start_time = Instant::now();
            let metadata_timeout = tool.metadata().timeout;
            let max_output_tokens = tool.metadata().max_output_tokens;
            let execute_fut = tool.execute(tool_call.input.clone(), &ctx);
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
                            &tx,
                            RuntimeEvent::ToolCallFailed {
                                tool: tool_call.name.clone(),
                                error: "tool execution timed out".into(),
                            },
                        )
                        .await;
                        tool_results.push(ContentBlock::ToolResult {
                            tool_use_id: tool_call.id.clone(),
                            content: json!({"error": "tool execution timed out"}),
                        });
                        budget.record_tool_call();
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
                    if config.runtime.tool_search_enabled && tool_call.name == "search_tools" {
                        append_searched_tool_defs(&mut tool_defs, &value);
                    }
                    let duration = start_time.elapsed();
                    emit(
                        &tx,
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
                }
                Ok(ToolOutput::Structured {
                    model_output,
                    details,
                }) => {
                    let mut model_output = model_output;
                    let mut details = details;
                    if let Some(max_tokens) = max_output_tokens {
                        model_output = truncate_output(model_output, max_tokens);
                        details = truncate_output(details, max_tokens);
                    }
                    let duration = start_time.elapsed();
                    emit(
                        &tx,
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
                }
                Ok(ToolOutput::AgentDelegate(delegate)) => {
                    let (model_output, details) = execute_agent_delegate(
                        run_id,
                        &config,
                        &tx,
                        &mut budget,
                        *delegate,
                        approval_bus.clone(),
                        cancel_token.clone(),
                    )
                    .await;
                    let duration = start_time.elapsed();
                    emit(
                        &tx,
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
                }
                Ok(ToolOutput::AsyncJob(handle)) => {
                    emit(
                        &tx,
                        RuntimeEvent::AsyncToolStarted {
                            tool: tool_call.name.clone(),
                            job_id: handle.job_id.clone(),
                        },
                    )
                    .await;

                    let async_result =
                        poll_async_job(&tx, &tool_call.name, &handle, start_time, &webhook_runtime)
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
                }
                Err(e) => {
                    let duration = start_time.elapsed();
                    emit(
                        &tx,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: e.message.clone(),
                        },
                    )
                    .await;
                    telemetry::record_tool_error(&tool_call.name, source_label, duration);
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": e.message}),
                    });
                }
            }

            budget.record_tool_call();
        }

        messages.push(Message {
            role: Role::User,
            content: tool_results,
        });

        step += 1;
    }
}
