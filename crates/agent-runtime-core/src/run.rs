use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::budget::{BudgetConfig, BudgetGuard, BudgetUsage};
use crate::events::RuntimeEvent;
use crate::telemetry;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelResponse, ModelSpec, ModelStreamChunk,
    RequestOptions, Role, StopReason,
};
use crate::tool::async_job::{JobHandle, JobStatus};
use crate::tool::builtin::ReadFileTool;
use crate::tool::code_exec::CodeExecutionMcpServer;
use crate::tool::mcp::{
    McpClient, McpHttpClient, McpServerConfig, McpStdioClient, McpTool, McpTransport,
};
use crate::tool::registry::ToolRegistry;
use crate::tool::search::SearchToolsTool;
use crate::tool::{AgentDelegate, Tool, ToolCall, ToolContext, ToolDef, ToolOutput};

use crate::skill::bundled_tool::SkillBundledTool;
use crate::skill::executor::BareSubprocessExecutor;
use crate::skill::{CapabilityValidator, SkillScanner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub uuid::Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: String,
    pub model: ModelSpec,
    #[serde(default)]
    pub request_options: RequestOptions,
    pub budget: BudgetConfig,
    pub max_steps: u32,
    pub allowed_skills: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub tool_search_enabled: bool,
    #[serde(default)]
    pub compaction_threshold: Option<f32>,
    #[serde(default = "default_recent_messages")]
    pub compaction_recent_messages: usize,
    #[serde(default)]
    pub webhook_enabled: bool,
    #[serde(default)]
    pub code_execution_enabled: bool,
    #[serde(default)]
    pub skills_dir: Option<String>,
    #[serde(default)]
    pub run_depth: u32,
}

fn default_recent_messages() -> usize {
    10
}

#[derive(Serialize, Deserialize)]
pub struct RunState {
    pub run_id: RunId,
    pub schema_version: String,
    pub config: AgentConfig,
    pub messages: Vec<Message>,
    #[serde(skip)]
    pub available_tools: Vec<Arc<dyn Tool>>,
    pub step: u32,
    pub status: RunStatus,
    pub budget_used: BudgetUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RunStatus {
    Running,
    WaitingForApproval {
        tool_call: ToolCall,
    },
    WaitingForAsyncTool {
        tool_call: ToolCall,
        job_handle: JobHandle,
        #[serde(skip, default = "Instant::now")]
        since: Instant,
    },
    Completed {
        output: Value,
    },
    Failed {
        error: String,
    },
    Aborted,
}

pub type EventReceiver = mpsc::Receiver<RuntimeEvent>;

struct WebhookRuntime {
    base_url: String,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<JobStatus>>>>,
    abort_handle: tokio::task::AbortHandle,
}

impl Drop for WebhookRuntime {
    fn drop(&mut self) {
        self.abort_handle.abort();
    }
}

/// Shared slot for a pending approval oneshot sender.  When a tool
/// requires approval, the run loop creates a oneshot pair, stores the
/// sender here, emits `ApprovalRequested`, and awaits the receiver.
/// `respond_approval` takes the sender out of the slot and sends the
/// verdict.  This guarantees that an approval response can only be
/// consumed by the request it was intended for.
type ApprovalSlot = Arc<Mutex<Option<oneshot::Sender<bool>>>>;

pub struct RunHandle {
    pub run_id: RunId,
    task: tokio::task::JoinHandle<()>,
    pending_approval: ApprovalSlot,
    active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
}

impl RunHandle {
    pub async fn wait(self) {
        let _ = self.task.await;
    }

    /// Route an approval response to the matching active run.
    ///
    /// If `run_id` matches this handle's own `run_id`, the approval goes
    /// to the root run.  If it matches a currently-active child run, the
    /// approval is forwarded there.  Returns `Err` if no approval is
    /// pending for the given `run_id` or the run is unknown.
    pub async fn respond_approval(&self, run_id: RunId, approved: bool) -> Result<(), String> {
        let slot = if run_id == self.run_id {
            &self.pending_approval
        } else {
            let children = self.active_children.lock().await;
            let child_slot = children.get(&run_id).cloned();
            return match child_slot {
                Some(slot) => take_and_send(&slot, approved, run_id).await,
                None => Err(format!(
                    "unknown run_id {run_id}: no active run or child with that id"
                )),
            };
        };
        take_and_send(slot, approved, run_id).await
    }
}

async fn take_and_send(slot: &ApprovalSlot, approved: bool, run_id: RunId) -> Result<(), String> {
    let sender = slot.lock().await.take();
    match sender {
        Some(tx) => tx
            .send(approved)
            .map_err(|_| format!("run {run_id} is no longer waiting for approval")),
        None => Err(format!("no approval pending for run {run_id}")),
    }
}

pub struct AgentRun;

pub struct SubAgentRuntime;

impl SubAgentRuntime {
    pub fn cap_budget(requested: &BudgetConfig, parent_remaining: &BudgetConfig) -> BudgetConfig {
        BudgetConfig {
            max_tokens: min_option(requested.max_tokens, parent_remaining.max_tokens),
            max_tool_calls: min_option(requested.max_tool_calls, parent_remaining.max_tool_calls),
            max_duration: min_option(requested.max_duration, parent_remaining.max_duration),
            max_cost_usd: min_option_f64(requested.max_cost_usd, parent_remaining.max_cost_usd),
        }
    }
}

fn truncate_output(value: Value, max_tokens: u64) -> Value {
    let max_bytes = max_tokens as usize * 4;
    match value {
        Value::String(s) if s.len() > max_bytes => {
            let truncated = truncate_str_utf8_safe(&s, max_bytes);
            Value::String(format!("{truncated}\n[output truncated]"))
        }
        other => {
            let serialized = serde_json::to_string(&other).unwrap_or_default();
            if serialized.len() > max_bytes {
                let truncated = truncate_str_utf8_safe(&serialized, max_bytes);
                Value::String(format!("{truncated}\n[output truncated]"))
            } else {
                other
            }
        }
    }
}

/// Truncate a string to at most `max_bytes` bytes without splitting a
/// multi-byte UTF-8 character.  The returned slice always ends on a
/// valid char boundary.
fn truncate_str_utf8_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // floor_char_boundary stabilised in Rust 1.82 — we inline the logic
    // for toolchain compatibility.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn narrow_permission_list(parent: &Option<Vec<String>>, requested: &[String]) -> Vec<String> {
    match parent {
        None => requested.to_vec(),
        Some(parent_list) => requested
            .iter()
            .filter(|name| parent_list.contains(name))
            .cloned()
            .collect(),
    }
}

fn min_option<T: Ord + Copy>(requested: Option<T>, remaining: Option<T>) -> Option<T> {
    match (requested, remaining) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn min_option_f64(requested: Option<f64>, remaining: Option<f64>) -> Option<f64> {
    match (requested, remaining) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

async fn register_skills(
    skills_dir: &str,
    allowed_skills: &Option<Vec<String>>,
    registry: &mut ToolRegistry,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> Result<Option<Arc<ReadFileTool>>, String> {
    use std::path::Path;

    let dir = Path::new(skills_dir);
    let manifests = SkillScanner::scan(dir).map_err(|e| format!("skill scan failed: {e}"))?;
    if manifests.is_empty() {
        return Ok(None);
    }

    let read_file_tool = Arc::new(ReadFileTool::new());
    let executor: Arc<dyn crate::skill::executor::ScriptExecutor> =
        Arc::new(BareSubprocessExecutor::new());

    for manifest in &manifests {
        // Filter by allowed_skills
        if let Some(ref allowed) = allowed_skills {
            if !allowed.contains(&manifest.name) {
                continue;
            }
        }

        // Emit SkillMissingCapabilities warning if applicable
        if CapabilityValidator::missing_capabilities_warning(manifest) {
            emit(
                tx,
                RuntimeEvent::SkillMissingCapabilities {
                    skill_name: manifest.name.clone(),
                },
            )
            .await;
        }

        // Register SKILL.md path with read_file for telemetry
        let skill_md_path = manifest.path.join("SKILL.md");
        if skill_md_path.exists() {
            read_file_tool
                .register_skill(manifest.name.clone(), skill_md_path)
                .await;
        }

        // Register each bundled tool
        for tool_def in &manifest.bundled_tools {
            let bundled = SkillBundledTool::new_with_options(
                tool_def,
                manifest.path.clone(),
                manifest.name.clone(),
                manifest.dependencies.clone(),
                manifest.capabilities.clone(),
                Arc::clone(&executor),
            )
            .map_err(|e| {
                format!(
                    "failed to create bundled tool '{}' for skill '{}': {}",
                    tool_def.name, manifest.name, e.message
                )
            })?;

            registry.register(Arc::new(bundled)).map_err(|e| {
                format!(
                    "duplicate tool name '{}' from skill '{}': {}",
                    tool_def.name, manifest.name, e
                )
            })?;
        }
    }

    // Register read_file tool for skill telemetry
    registry
        .register(read_file_tool.clone() as Arc<dyn Tool>)
        .map_err(|e| format!("failed to register read_file tool: {e}"))?;

    Ok(Some(read_file_tool))
}

impl AgentRun {
    pub fn start(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        let run_id = RunId::new();
        let (event_tx, event_rx) = mpsc::channel(256);
        let pending_approval: ApprovalSlot = Arc::new(Mutex::new(None));
        let active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let approval_for_loop = Arc::clone(&pending_approval);
        let children_for_loop = Arc::clone(&active_children);
        let task = tokio::spawn(async move {
            run_loop(
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

async fn emit(tx: &mpsc::Sender<RuntimeEvent>, event: RuntimeEvent) {
    let _ = tx.send(event).await;
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    run_id: RunId,
    config: AgentConfig,
    input: String,
    model: Arc<dyn ModelAdapter>,
    mut registry: ToolRegistry,
    tx: mpsc::Sender<RuntimeEvent>,
    pending_approval: ApprovalSlot,
    active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
) {
    emit(&tx, RuntimeEvent::RunStarted { run_id }).await;

    let webhook_runtime = if config.webhook_enabled {
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

    if config.code_execution_enabled {
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
    if let Some(ref skills_dir) = config.skills_dir {
        if let Err(error) =
            register_skills(skills_dir, &config.allowed_skills, &mut registry, &tx).await
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
    let mut registry = registry.filter_by_allowed(&config.allowed_tools);

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
    let mut tool_defs = if config.tool_search_enabled {
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
        if step >= config.max_steps {
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: "max_steps_reached".into(),
                },
            )
            .await;
            return;
        }

        if let Some(_violation) = budget.check() {
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
                    error: "budget_exceeded".into(),
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
                &config.request_options,
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
                // Create a oneshot for this specific approval request
                // and store the sender so RunHandle can deliver it.
                let (atx, arx) = oneshot::channel();
                {
                    let mut slot = pending_approval.lock().await;
                    *slot = Some(atx);
                }
                emit(
                    &tx,
                    RuntimeEvent::ApprovalRequested {
                        tool_call: tool_call.clone(),
                    },
                )
                .await;

                let approved = arx.await.unwrap_or(false);

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

            let _tool_span =
                telemetry::tool_execute_span(&tool_call.name, source_label);

            let ctx = ToolContext {
                run_id,
                run_depth: config.run_depth,
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
                    if value.get("__sub_agent_request").and_then(Value::as_bool) == Some(true) {
                        value = execute_sub_agent_request(
                            run_id,
                            &config,
                            &model,
                            &registry,
                            &tx,
                            &mut budget,
                            &value,
                            &active_children,
                        )
                        .await;
                    }
                    if config.tool_search_enabled && tool_call.name == "search_tools" {
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
                        &active_children,
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

#[allow(clippy::too_many_arguments)]
async fn execute_sub_agent_request(
    parent_run_id: RunId,
    parent_config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
    registry: &ToolRegistry,
    tx: &mpsc::Sender<RuntimeEvent>,
    parent_budget: &mut BudgetGuard,
    request: &Value,
    active_children: &Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
) -> Value {
    let child_run_id = RunId::new();
    if parent_config.run_depth >= 3 {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: "max_run_depth_exceeded".into(),
            },
        )
        .await;
        return json!({"error": "max_run_depth_exceeded"});
    }
    let remaining = parent_budget.remaining_config();
    if remaining.max_tokens == Some(0)
        || remaining.max_tool_calls == Some(0)
        || remaining.max_duration == Some(Duration::ZERO)
        || remaining.max_cost_usd == Some(0.0)
    {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: "parent_budget_exhausted".into(),
            },
        )
        .await;
        return json!({"error": "parent_budget_exhausted"});
    }

    let requested_budget = request
        .get("config")
        .and_then(|config| config.get("budget"))
        .map(parse_budget_config)
        .unwrap_or_else(|| remaining.clone());
    let mut child_config = parent_config.clone();
    child_config.budget = SubAgentRuntime::cap_budget(&requested_budget, &remaining);
    child_config.run_depth = parent_config.run_depth + 1;
    if let Some(requested_tools) = request
        .get("config")
        .and_then(|config| config.get("allowed_tools"))
        .and_then(Value::as_array)
    {
        let requested: Vec<String> = requested_tools
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        child_config.allowed_tools = Some(narrow_permission_list(
            &parent_config.allowed_tools,
            &requested,
        ));
    }
    if let Some(requested_skills) = request
        .get("config")
        .and_then(|config| config.get("allowed_skills"))
        .and_then(Value::as_array)
    {
        let requested: Vec<String> = requested_skills
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        child_config.allowed_skills = Some(narrow_permission_list(
            &parent_config.allowed_skills,
            &requested,
        ));
    }
    let input = request
        .get("input")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let child_registry = registry.filter_by_allowed(&child_config.allowed_tools);
    let (handle, mut child_rx) =
        AgentRun::start(child_config, input, Arc::clone(model), child_registry);
    let actual_child_run_id = handle.run_id;

    // Register child approval slot so parent RunHandle can route
    // approval responses to the child run.
    {
        let mut children = active_children.lock().await;
        children.insert(actual_child_run_id, Arc::clone(&handle.pending_approval));
    }

    emit(
        tx,
        RuntimeEvent::SubAgentStarted {
            parent_run_id,
            child_run_id: actual_child_run_id,
            config_summary: json!({
                "run_depth": parent_config.run_depth + 1,
                "budget": request.get("config").and_then(|config| config.get("budget")).cloned().unwrap_or(Value::Null),
            }),
        },
    )
    .await;
    let mut child_usage = BudgetUsage::default();
    let mut output = Value::Null;
    let mut failed = None;

    let child_depth = parent_config.run_depth + 1;
    while let Some(event) = child_rx.recv().await {
        match &event {
            RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                child_usage.tokens_used += tokens.input_tokens + tokens.output_tokens;
                // Propagate to parent budget immediately so the parent
                // guard reflects child consumption in real time.
                let incremental = BudgetUsage {
                    tokens_used: tokens.input_tokens + tokens.output_tokens,
                    tool_calls_used: 0,
                    cost_usd: 0.0,
                };
                parent_budget.record_external_usage(&incremental);
            }
            RuntimeEvent::ToolCallCompleted { .. } => {
                child_usage.tool_calls_used += 1;
                let incremental = BudgetUsage {
                    tokens_used: 0,
                    tool_calls_used: 1,
                    cost_usd: 0.0,
                };
                parent_budget.record_external_usage(&incremental);
            }
            RuntimeEvent::RunCompleted {
                output: child_output,
            } => {
                output = child_output.clone();
            }
            RuntimeEvent::RunFailed { error } => {
                failed = Some(error.clone());
            }
            _ => {}
        }
        // Wrap child events with identity metadata so consumers can
        // distinguish root vs child run events
        emit(
            tx,
            RuntimeEvent::ChildRunEvent {
                child_run_id: actual_child_run_id,
                run_depth: child_depth,
                event: Box::new(event),
            },
        )
        .await;
    }
    handle.wait().await;

    // Deregister child from active children map
    {
        let mut children = active_children.lock().await;
        children.remove(&actual_child_run_id);
    }

    // Note: parent_budget was already updated incrementally above.
    // child_usage is kept for the SubAgentCompleted event payload.

    if let Some(error) = failed {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id: actual_child_run_id,
                error: error.clone(),
            },
        )
        .await;
        json!({"error": error})
    } else {
        emit(
            tx,
            RuntimeEvent::SubAgentCompleted {
                child_run_id: actual_child_run_id,
                output: output.clone(),
                budget_used: child_usage,
            },
        )
        .await;
        output
    }
}

async fn execute_agent_delegate(
    parent_run_id: RunId,
    parent_config: &AgentConfig,
    tx: &mpsc::Sender<RuntimeEvent>,
    parent_budget: &mut BudgetGuard,
    delegate: AgentDelegate,
    active_children: &Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
) -> (Value, Value) {
    let mut child_config = delegate.config.clone();
    let remaining = parent_budget.remaining_config();
    child_config.budget = SubAgentRuntime::cap_budget(&child_config.budget, &remaining);
    child_config.run_depth = parent_config.run_depth + 1;

    let (handle, mut child_rx) = AgentRun::start(
        child_config,
        delegate.input.clone(),
        Arc::clone(&delegate.model),
        delegate.registry.clone(),
    );
    let child_run_id = handle.run_id;

    {
        let mut children = active_children.lock().await;
        children.insert(child_run_id, Arc::clone(&handle.pending_approval));
    }

    emit(
        tx,
        RuntimeEvent::SubAgentStarted {
            parent_run_id,
            child_run_id,
            config_summary: json!({
                "run_depth": parent_config.run_depth + 1,
                "input": delegate.input,
            }),
        },
    )
    .await;

    let mut child_usage = BudgetUsage::default();
    let mut output = Value::Null;
    let mut failed = None;
    let child_depth = parent_config.run_depth + 1;

    while let Some(event) = child_rx.recv().await {
        match &event {
            RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                let tokens_used = tokens.input_tokens + tokens.output_tokens;
                child_usage.tokens_used += tokens_used;
                parent_budget.record_external_usage(&BudgetUsage {
                    tokens_used,
                    tool_calls_used: 0,
                    cost_usd: 0.0,
                });
            }
            RuntimeEvent::ToolCallCompleted { .. } => {
                child_usage.tool_calls_used += 1;
                parent_budget.record_external_usage(&BudgetUsage {
                    tokens_used: 0,
                    tool_calls_used: 1,
                    cost_usd: 0.0,
                });
            }
            RuntimeEvent::RunCompleted {
                output: child_output,
            } => {
                output = child_output.clone();
            }
            RuntimeEvent::RunFailed { error } => {
                failed = Some(error.clone());
            }
            _ => {}
        }
        emit(
            tx,
            RuntimeEvent::ChildRunEvent {
                child_run_id,
                run_depth: child_depth,
                event: Box::new(event),
            },
        )
        .await;
    }
    handle.wait().await;

    {
        let mut children = active_children.lock().await;
        children.remove(&child_run_id);
    }

    let details = if let Some(error) = failed {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: error.clone(),
            },
        )
        .await;
        json!({
            "child_run_id": child_run_id,
            "error": error,
            "budget_used": child_usage,
        })
    } else {
        emit(
            tx,
            RuntimeEvent::SubAgentCompleted {
                child_run_id,
                output: output.clone(),
                budget_used: child_usage.clone(),
            },
        )
        .await;
        json!({
            "child_run_id": child_run_id,
            "output": output,
            "budget_used": child_usage,
        })
    };

    let model_output = (delegate.output_mapper)(details.clone());
    (model_output, details)
}

fn parse_budget_config(value: &Value) -> BudgetConfig {
    BudgetConfig {
        max_tokens: value.get("max_tokens").and_then(Value::as_u64),
        max_tool_calls: value
            .get("max_tool_calls")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        max_duration: value
            .get("max_duration_secs")
            .and_then(Value::as_u64)
            .map(Duration::from_secs),
        max_cost_usd: value.get("max_cost_usd").and_then(Value::as_f64),
    }
}

async fn maybe_compact_context(
    config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
    messages: &mut Vec<Message>,
    tx: &mpsc::Sender<RuntimeEvent>,
    last_compaction_step: &mut Option<u32>,
    step: u32,
    usage: &crate::model::TokenUsage,
) {
    let Some(threshold) = config.compaction_threshold else {
        return;
    };
    let Some(context_window_size) = config.model.context_window_size else {
        return;
    };
    if !(0.0..=1.0).contains(&threshold) || context_window_size == 0 {
        return;
    }
    if let Some(last) = *last_compaction_step {
        if step.saturating_sub(last) < 5 {
            return;
        }
    }
    let used = usage.input_tokens.saturating_add(usage.output_tokens);
    if (used as f32 / context_window_size as f32) < threshold {
        return;
    }
    let recent_count = config.compaction_recent_messages;
    if messages.len() <= recent_count + 1 {
        return;
    }

    let system = messages
        .iter()
        .find(|message| matches!(message.role, Role::System))
        .cloned();
    let non_system: Vec<Message> = messages
        .iter()
        .filter(|message| !matches!(message.role, Role::System))
        .cloned()
        .collect();
    if non_system.len() <= recent_count {
        return;
    }
    let split_at = non_system.len() - recent_count;
    let old_messages = &non_system[..split_at];
    let recent_messages = non_system[split_at..].to_vec();
    let history = old_messages
        .iter()
        .map(|message| serde_json::to_string(message).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "以下是一次 AI agent 任务的历史对话记录。请用简洁的中文总结这段历史中发生的关键事件：\n完成了哪些工具调用、获取了哪些信息、做出了哪些决策。保留足够细节让 agent 能够继续任务。\n\n{history}"
    );
    let summary_response = model
        .complete(
            &[Message {
                role: Role::User,
                content: vec![ContentBlock::Text(prompt)],
            }],
            &[],
            &config.request_options,
            None,
        )
        .await;

    let response = match summary_response {
        Ok(response) => response,
        Err(error) => {
            emit(
                tx,
                RuntimeEvent::RuntimeWarning {
                    message: format!("context compaction failed: {}", error.message),
                },
            )
            .await;
            return;
        }
    };
    let summary = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    let mut compacted = Vec::new();
    if let Some(system) = system {
        compacted.push(system);
    }
    compacted.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text(format!("历史摘要：{summary}"))],
    });
    compacted.extend(recent_messages);
    let removed_messages = messages.len().saturating_sub(compacted.len());
    *messages = compacted;
    *last_compaction_step = Some(step);
    emit(
        tx,
        RuntimeEvent::ContextCompacted {
            removed_messages,
            summary_tokens: response.usage.output_tokens as u32,
        },
    )
    .await;
}

fn append_searched_tool_defs(tool_defs: &mut Vec<ToolDef>, value: &Value) {
    let Some(results) = value.as_array() else {
        return;
    };
    for result in results {
        let Some(name) = result.get("name").and_then(Value::as_str) else {
            continue;
        };
        if tool_defs.iter().any(|tool| tool.name == name) {
            continue;
        }
        tool_defs.push(ToolDef {
            name: name.to_string(),
            description: result
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            input_schema: result
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"})),
        });
    }
}

async fn poll_async_job(
    tx: &mpsc::Sender<RuntimeEvent>,
    tool_name: &str,
    handle: &JobHandle,
    start_time: Instant,
    webhook_runtime: &Option<WebhookRuntime>,
) -> Value {
    let timeout = handle.timeout;

    if let (Some(webhook), Some(runtime)) = (&handle.webhook, webhook_runtime) {
        let (webhook_tx, webhook_rx) = oneshot::channel();
        runtime
            .waiters
            .lock()
            .await
            .insert(webhook.expected_job_id.clone(), webhook_tx);
        let wait_for = handle.poll_interval * 3;
        match tokio::time::timeout(wait_for, webhook_rx).await {
            Ok(Ok(JobStatus::Completed(value))) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolCompleted {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        output: value.clone(),
                        elapsed: start_time.elapsed(),
                    },
                )
                .await;
                return value;
            }
            Ok(Ok(JobStatus::Failed(error))) => {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: error.clone(),
                    },
                )
                .await;
                return json!({"error": error});
            }
            Ok(Ok(JobStatus::Pending { progress, message })) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolProgress {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        status: JobStatus::Pending { progress, message },
                    },
                )
                .await;
            }
            Ok(Err(_)) | Err(_) => {}
        }
        runtime
            .waiters
            .lock()
            .await
            .remove(&webhook.expected_job_id);
    }

    loop {
        tokio::time::sleep(handle.poll_interval).await;

        if let Some(max) = timeout {
            if start_time.elapsed() > max {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: "async job timed out".into(),
                    },
                )
                .await;
                return json!({"error": "async job timed out"});
            }
        }

        let Some(poll) = &handle.poll else {
            emit(
                tx,
                RuntimeEvent::ToolCallFailed {
                    tool: tool_name.to_string(),
                    error: "async job has no polling fallback".into(),
                },
            )
            .await;
            return json!({"error": "async job has no polling fallback"});
        };

        match (poll)().await {
            Ok(JobStatus::Pending { progress, message }) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolProgress {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        status: JobStatus::Pending { progress, message },
                    },
                )
                .await;
            }
            Ok(JobStatus::Completed(value)) => {
                let elapsed = start_time.elapsed();
                emit(
                    tx,
                    RuntimeEvent::AsyncToolCompleted {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        output: value.clone(),
                        elapsed,
                    },
                )
                .await;
                return value;
            }
            Ok(JobStatus::Failed(err)) => {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: err.clone(),
                    },
                )
                .await;
                return json!({"error": err});
            }
            Err(e) => {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: e.message.clone(),
                    },
                )
                .await;
                return json!({"error": e.message});
            }
        }
    }
}

async fn start_webhook_server() -> Result<WebhookRuntime, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind webhook server: {e}"))?;
    let address = listener
        .local_addr()
        .map_err(|e| format!("failed to read webhook address: {e}"))?;
    let waiters: Arc<Mutex<HashMap<String, oneshot::Sender<JobStatus>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let waiters_for_task = Arc::clone(&waiters);

    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let waiters = Arc::clone(&waiters_for_task);
            tokio::spawn(async move {
                let mut buffer = vec![0; 16 * 1024];
                let mut read_total = 0usize;
                loop {
                    let Ok(read) = socket.read(&mut buffer[read_total..]).await else {
                        return;
                    };
                    if read == 0 {
                        break;
                    }
                    read_total += read;
                    let request = String::from_utf8_lossy(&buffer[..read_total]);
                    if let Some(header_end) =
                        request.find("\r\n\r\n").or_else(|| request.find("\n\n"))
                    {
                        let header = &request[..header_end];
                        let body_start = if request[header_end..].starts_with("\r\n\r\n") {
                            header_end + 4
                        } else {
                            header_end + 2
                        };
                        let content_length = header
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        if read_total >= body_start + content_length {
                            break;
                        }
                    }
                    if read_total == buffer.len() {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&buffer[..read_total]);
                let Some(first_line) = request.lines().next() else {
                    return;
                };
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() < 2 || parts[0] != "POST" {
                    let _ = write_http_response(&mut socket, 405, "method not allowed").await;
                    return;
                }
                let Some(job_id) = parts[1].strip_prefix("/webhooks/async-job/") else {
                    let _ = write_http_response(&mut socket, 404, "not found").await;
                    return;
                };
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .or_else(|| request.split("\n\n").nth(1))
                    .unwrap_or_default();
                let parsed: Value = match serde_json::from_str(body) {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = write_http_response(&mut socket, 400, "bad request").await;
                        return;
                    }
                };
                let status = match parsed.get("status").and_then(Value::as_str) {
                    Some("completed") => {
                        JobStatus::Completed(parsed.get("result").cloned().unwrap_or(Value::Null))
                    }
                    Some("failed") => JobStatus::Failed(
                        parsed
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("webhook job failed")
                            .to_string(),
                    ),
                    _ => JobStatus::Pending {
                        progress: parsed
                            .get("progress")
                            .and_then(Value::as_f64)
                            .map(|value| value as f32),
                        message: parsed
                            .get("message")
                            .and_then(Value::as_str)
                            .map(String::from),
                    },
                };
                if let Some(waiter) = waiters.lock().await.remove(job_id) {
                    let _ = waiter.send(status);
                }
                let _ = write_http_response(&mut socket, 200, "ok").await;
            });
        }
    });

    Ok(WebhookRuntime {
        base_url: format!("http://{address}"),
        waiters,
        abort_handle: task.abort_handle(),
    })
}

async fn write_http_response(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    socket
        .write_all(
            format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
}

async fn connect_mcp_servers(
    config: &AgentConfig,
    registry: &mut ToolRegistry,
) -> Result<(), crate::tool::mcp::McpError> {
    for server in &config.mcp_servers {
        match &server.transport {
            McpTransport::Stdio { command, args } => {
                let client = Arc::new(McpStdioClient::connect_owned(command, args).await?);
                let tools = client.list_tools().await?;
                for def in tools {
                    registry
                        .register(Arc::new(McpTool::new(
                            server.server_id.clone(),
                            def,
                            McpClient::Stdio(Arc::clone(&client)),
                        )))
                        .map_err(|e| crate::tool::mcp::McpError {
                            message: e.to_string(),
                            code: Some("registry_error".into()),
                        })?;
                }
            }
            McpTransport::StreamableHttp { url, auth } => {
                let client = Arc::new(McpHttpClient::connect(url, auth.clone()).await?);
                let tools = client.list_tools().await?;
                for def in tools {
                    registry
                        .register(Arc::new(McpTool::new(
                            server.server_id.clone(),
                            def,
                            McpClient::Http(Arc::clone(&client)),
                        )))
                        .map_err(|e| crate::tool::mcp::McpError {
                            message: e.to_string(),
                            code: Some("registry_error".into()),
                        })?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::BudgetConfig;
    use crate::model::{ModelCapabilities, ModelError, RequestOptions, StreamEvent, TokenUsage};
    use crate::tool::{JsonSchema, ToolDef, ToolError, ToolMetadata, ToolSource};
    use std::sync::atomic::{AtomicU32, Ordering};

    struct FakeModelAdapter {
        call_count: AtomicU32,
    }

    impl FakeModelAdapter {
        fn final_answer() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelAdapter for FakeModelAdapter {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Text {
                        delta: "hello".into(),
                    })
                    .await;
            }
            let usage = TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            if count == 0 {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("hello".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            }
        }
    }

    fn test_config() -> AgentConfig {
        AgentConfig {
            system_prompt: "you are helpful".into(),
            model: ModelSpec {
                provider: "test".into(),
                model: "test".into(),
                api_key_env: None,
                api_url: None,
                max_tokens: None,
                context_window_size: None,
            },
            request_options: RequestOptions::default(),
            budget: BudgetConfig {
                max_tokens: None,
                max_tool_calls: None,
                max_duration: None,
                max_cost_usd: None,
            },
            max_steps: 10,
            allowed_skills: None,
            allowed_tools: None,
            mcp_servers: vec![],
            tool_search_enabled: false,
            compaction_threshold: None,
            compaction_recent_messages: default_recent_messages(),
            webhook_enabled: false,
            code_execution_enabled: false,
            skills_dir: None,
            run_depth: 0,
        }
    }

    #[tokio::test]
    async fn run_loop_final_answer() {
        let model = Arc::new(FakeModelAdapter::final_answer());
        let registry = ToolRegistry::new();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(matches!(events[0], RuntimeEvent::RunStarted { .. }));
        assert!(matches!(
            events[1],
            RuntimeEvent::ModelCallStarted { step: 0 }
        ));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
    }

    #[tokio::test]
    async fn run_loop_max_steps() {
        let model = Arc::new(FakeModelAdapter::final_answer());
        let registry = ToolRegistry::new();

        let mut config = test_config();
        config.max_steps = 0;

        let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(events.iter().any(|e| matches!(
            e,
            RuntimeEvent::RunFailed { error } if error == "max_steps_reached"
        )));
    }

    struct ToolCallModelAdapter;

    #[async_trait::async_trait]
    impl ModelAdapter for ToolCallModelAdapter {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });

            let usage = TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "echo".into(),
                        input: json!({"text": "hello"}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    struct FakeTool {
        name: &'static str,
        requires_approval: bool,
    }

    impl FakeTool {
        fn echo() -> Self {
            Self {
                name: "echo",
                requires_approval: false,
            }
        }

        fn guarded(name: &'static str) -> Self {
            Self {
                name,
                requires_approval: true,
            }
        }
    }

    #[async_trait::async_trait]
    impl Tool for FakeTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "fake tool"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            if self.requires_approval {
                &ToolMetadata {
                    side_effect: true,
                    requires_approval: true,
                    cost_hint: None,
                    timeout: None,
                    max_output_tokens: None,
                    source: ToolSource::InProcess,
                }
            } else {
                &ToolMetadata {
                    side_effect: false,
                    requires_approval: false,
                    cost_hint: None,
                    timeout: None,
                    max_output_tokens: None,
                    source: ToolSource::InProcess,
                }
            }
        }
        async fn execute(
            &self,
            input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Immediate(input))
        }
    }

    #[tokio::test]
    async fn run_loop_with_tool_call() {
        let model = Arc::new(ToolCallModelAdapter);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { tool, .. } if tool == "echo")));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "echo")));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
    }

    struct StructuredToolModelAdapter;

    #[async_trait::async_trait]
    impl ModelAdapter for StructuredToolModelAdapter {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let usage = TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            let tool_result = messages.iter().find_map(|message| {
                message.content.iter().find_map(|block| match block {
                    ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                    _ => None,
                })
            });

            if let Some(content) = tool_result {
                assert_eq!(content, json!({"summary": "compact for model"}));
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "research".into(),
                        input: json!({"question": "q"}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    struct StructuredTool;

    #[async_trait::async_trait]
    impl Tool for StructuredTool {
        fn name(&self) -> &str {
            "research"
        }
        fn description(&self) -> &str {
            "structured output tool"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            }
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Structured {
                model_output: json!({"summary": "compact for model"}),
                details: json!({
                    "summary": "compact for model",
                    "raw_results": ["large detail only for events"]
                }),
            })
        }
    }

    #[tokio::test]
    async fn structured_tool_output_sends_details_to_events_and_model_output_to_model() {
        let model = Arc::new(StructuredToolModelAdapter);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(StructuredTool)).unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        let detail_output = events.iter().find_map(|event| match event {
            RuntimeEvent::ToolCallCompleted { tool, output, .. } if tool == "research" => {
                Some(output)
            }
            _ => None,
        });
        assert_eq!(
            detail_output,
            Some(&json!({
                "summary": "compact for model",
                "raw_results": ["large detail only for events"]
            }))
        );
    }

    struct ApprovalModelAdapter;

    #[async_trait::async_trait]
    impl ModelAdapter for ApprovalModelAdapter {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "write_file".into(),
                        input: json!({}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn approval_gate_approved() {
        let model = Arc::new(ApprovalModelAdapter);
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(FakeTool::guarded("write_file")))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        loop {
            match rx.recv().await {
                Some(RuntimeEvent::ApprovalRequested { .. }) => {
                    events.push(RuntimeEvent::ApprovalRequested {
                        tool_call: ToolCall {
                            id: String::new(),
                            name: String::new(),
                            input: json!(null),
                        },
                    });
                    handle.respond_approval(handle.run_id, true).await.unwrap();
                }
                Some(event) => events.push(event),
                None => break,
            }
        }
        handle.wait().await;

        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ApprovalRequested { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ApprovalGranted { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { .. })));
    }

    #[tokio::test]
    async fn approval_gate_denied() {
        let model = Arc::new(ApprovalModelAdapter);
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(FakeTool::guarded("write_file")))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        loop {
            match rx.recv().await {
                Some(RuntimeEvent::ApprovalRequested { .. }) => {
                    events.push(RuntimeEvent::ApprovalRequested {
                        tool_call: ToolCall {
                            id: String::new(),
                            name: String::new(),
                            input: json!(null),
                        },
                    });
                    handle.respond_approval(handle.run_id, false).await.unwrap();
                }
                Some(event) => events.push(event),
                None => break,
            }
        }
        handle.wait().await;

        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ApprovalDenied { .. })));
        assert!(!events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { .. })));
    }

    struct AsyncTool {
        polls_until_done: AtomicU32,
    }

    #[async_trait::async_trait]
    impl Tool for AsyncTool {
        fn name(&self) -> &str {
            "async_op"
        }
        fn description(&self) -> &str {
            "async operation"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            }
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            let remaining = Arc::new(AtomicU32::new(self.polls_until_done.load(Ordering::SeqCst)));
            let poll_fn = {
                let remaining = Arc::clone(&remaining);
                move || {
                    let remaining = Arc::clone(&remaining);
                    Box::pin(async move {
                        let left = remaining.fetch_sub(1, Ordering::SeqCst);
                        if left <= 1 {
                            Ok(JobStatus::Completed(json!("async_result")))
                        } else {
                            Ok(JobStatus::Pending {
                                progress: Some(1.0 - (left as f32 / 3.0)),
                                message: Some("working".into()),
                            })
                        }
                    })
                        as std::pin::Pin<
                            Box<
                                dyn std::future::Future<Output = Result<JobStatus, ToolError>>
                                    + Send,
                            >,
                        >
                }
            };
            Ok(ToolOutput::AsyncJob(JobHandle {
                job_id: "job-1".into(),
                poll: Some(Arc::new(poll_fn)),
                poll_interval: std::time::Duration::from_millis(1),
                timeout: None,
                webhook: None,
            }))
        }
    }

    struct AsyncToolModelAdapter;

    #[async_trait::async_trait]
    impl ModelAdapter for AsyncToolModelAdapter {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "async_op".into(),
                        input: json!({}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn async_job_polling() {
        let model = Arc::new(AsyncToolModelAdapter);
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(AsyncTool {
                polls_until_done: AtomicU32::new(3),
            }))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::AsyncToolStarted { tool, job_id } if tool == "async_op" && job_id == "job-1")));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::AsyncToolProgress { .. })));
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::AsyncToolCompleted { tool, .. } if tool == "async_op")
        ));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
    }

    struct ToolSearchModel {
        call_count: AtomicU32,
    }

    #[async_trait::async_trait]
    impl ModelAdapter for ToolSearchModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[Message],
            tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            if count == 0 {
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].name, "search_tools");
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "search_1".into(),
                        name: "search_tools".into(),
                        input: json!({"query": "async operation"}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            } else {
                assert!(tools.iter().any(|tool| tool.name == "async_op"));
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn tool_search_enabled_loads_schemas_progressively() {
        let mut config = test_config();
        config.tool_search_enabled = true;
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(AsyncTool {
                polls_until_done: AtomicU32::new(1),
            }))
            .unwrap();
        let model = Arc::new(ToolSearchModel {
            call_count: AtomicU32::new(0),
        });
        let (handle, mut rx) = AgentRun::start(config, "find tool".into(), model, registry);
        while rx.recv().await.is_some() {}
        handle.wait().await;
    }

    struct CompactingModel {
        call_count: AtomicU32,
    }

    #[async_trait::async_trait]
    impl ModelAdapter for CompactingModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            let usage = if count == 0 {
                TokenUsage {
                    input_tokens: 90,
                    output_tokens: 20,
                    ..Default::default()
                }
            } else {
                TokenUsage {
                    input_tokens: 1,
                    output_tokens: 3,
                    ..Default::default()
                }
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            if messages
                .iter()
                .any(|message| matches!(message.role, Role::User)
                    && message.content.iter().any(|block| matches!(block, ContentBlock::Text(text) if text.contains("历史对话记录"))))
            {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("摘要".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn context_compaction_emits_event() {
        let mut config = test_config();
        config.compaction_threshold = Some(0.5);
        config.compaction_recent_messages = 0;
        config.model.context_window_size = Some(100);
        let model = Arc::new(CompactingModel {
            call_count: AtomicU32::new(0),
        });
        let registry = ToolRegistry::new();
        let (handle, mut rx) = AgentRun::start(config, "compact".into(), model, registry);
        let mut saw_compacted = false;
        while let Some(event) = rx.recv().await {
            if matches!(event, RuntimeEvent::ContextCompacted { .. }) {
                saw_compacted = true;
            }
        }
        handle.wait().await;
        assert!(saw_compacted);
    }

    struct WebhookTool;

    #[async_trait::async_trait]
    impl Tool for WebhookTool {
        fn name(&self) -> &str {
            "webhook_tool"
        }
        fn description(&self) -> &str {
            "webhook async tool"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            }
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            let job_id = uuid::Uuid::new_v4().to_string();
            let url = format!(
                "{}/webhooks/async-job/{}",
                ctx.webhook_base_url.as_ref().ok_or_else(|| ToolError {
                    message: "missing webhook base url".into(),
                    code: None,
                })?,
                job_id
            );
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let _ = reqwest::Client::new()
                    .post(url)
                    .json(&json!({"status": "completed", "result": {"ok": true}}))
                    .send()
                    .await;
            });
            Ok(ToolOutput::AsyncJob(JobHandle {
                job_id: job_id.clone(),
                poll: None,
                poll_interval: std::time::Duration::from_secs(1),
                timeout: Some(std::time::Duration::from_secs(5)),
                webhook: Some(crate::tool::async_job::WebhookConfig {
                    expected_job_id: job_id,
                }),
            }))
        }
    }

    struct WebhookModel {
        call_count: AtomicU32,
    }

    #[async_trait::async_trait]
    impl ModelAdapter for WebhookModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            if self.call_count.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "webhook_1".into(),
                        name: "webhook_tool".into(),
                        input: json!({}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            } else {
                assert!(messages.iter().any(|message| {
                    message.content.iter().any(|block| {
                        matches!(block, ContentBlock::ToolResult { content, .. } if content["ok"] == true)
                    })
                }));
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn webhook_async_job_completes_without_polling() {
        let mut config = test_config();
        config.webhook_enabled = true;
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(WebhookTool)).unwrap();
        let model = Arc::new(WebhookModel {
            call_count: AtomicU32::new(0),
        });
        let (handle, mut rx) = AgentRun::start(config, "run webhook".into(), model, registry);
        let mut completed = false;
        while let Some(event) = rx.recv().await {
            if matches!(event, RuntimeEvent::AsyncToolCompleted { .. }) {
                completed = true;
            }
        }
        handle.wait().await;
        assert!(completed);
    }

    struct AllowedToolsModel {
        target_tool: String,
    }

    #[async_trait::async_trait]
    impl ModelAdapter for AllowedToolsModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: self.target_tool.clone(),
                        input: json!({}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn allowed_tools_filters_visibility_and_permits_execution() {
        let mut config = test_config();
        config.allowed_tools = Some(vec!["echo".into()]);

        let model = Arc::new(AllowedToolsModel {
            target_tool: "echo".into(),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();
        registry
            .register(Arc::new(FakeTool {
                name: "secret",
                requires_approval: false,
            }))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "echo")));
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
    }

    #[tokio::test]
    async fn allowed_tools_denies_disallowed_tool_by_name() {
        let mut config = test_config();
        config.allowed_tools = Some(vec!["echo".into()]);

        let model = Arc::new(AllowedToolsModel {
            target_tool: "secret".into(),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();
        registry
            .register(Arc::new(FakeTool {
                name: "secret",
                requires_approval: false,
            }))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool not allowed")
            ),
            "should emit ToolCallFailed with 'tool not allowed'"
        );
        assert!(
            !events.iter().any(
                |e| matches!(e, RuntimeEvent::ToolCallStarted { tool, .. } if tool == "secret")
            ),
            "ToolCallStarted must not be emitted for denied-by-policy tools"
        );
    }

    #[tokio::test]
    async fn allowed_tools_empty_list_denies_all() {
        let mut config = test_config();
        config.allowed_tools = Some(vec![]);

        let model = Arc::new(AllowedToolsModel {
            target_tool: "echo".into(),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();

        let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool not allowed")
            ),
        );
    }

    #[tokio::test]
    async fn allowed_tools_none_permits_all() {
        let mut config = test_config();
        config.allowed_tools = None;

        let model = Arc::new(AllowedToolsModel {
            target_tool: "echo".into(),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();

        let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "echo")));
    }

    #[test]
    fn narrow_permission_list_intersects_with_parent() {
        let parent = Some(vec!["a".into(), "b".into()]);
        let requested = vec!["b".into(), "c".into()];
        let result = narrow_permission_list(&parent, &requested);
        assert_eq!(result, vec!["b".to_string()]);
    }

    #[test]
    fn narrow_permission_list_none_parent_allows_all_requested() {
        let parent = None;
        let requested = vec!["x".into(), "y".into()];
        let result = narrow_permission_list(&parent, &requested);
        assert_eq!(result, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn narrow_permission_list_expansion_rejected() {
        let parent = Some(vec!["a".into()]);
        let requested = vec!["a".into(), "b".into()];
        let result = narrow_permission_list(&parent, &requested);
        assert_eq!(result, vec!["a".to_string()]);
    }

    struct SlowTool {
        metadata: ToolMetadata,
    }

    impl SlowTool {
        fn new() -> Self {
            Self {
                metadata: ToolMetadata {
                    side_effect: false,
                    requires_approval: false,
                    cost_hint: None,
                    timeout: Some(Duration::from_millis(50)),
                    max_output_tokens: None,
                    source: ToolSource::InProcess,
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl Tool for SlowTool {
        fn name(&self) -> &str {
            "slow"
        }
        fn description(&self) -> &str {
            "sleeps forever"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &self.metadata
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(ToolOutput::Immediate(json!("should not reach")))
        }
    }

    struct SlowToolModel;

    #[async_trait::async_trait]
    impl ModelAdapter for SlowToolModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "slow".into(),
                        input: json!({}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn tool_metadata_timeout_enforced() {
        let model = Arc::new(SlowToolModel);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(SlowTool::new())).unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool execution timed out")
            ),
            "should emit ToolCallFailed with timeout error"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
            "run should continue after timeout"
        );
    }

    struct BigOutputTool {
        metadata: ToolMetadata,
    }

    impl BigOutputTool {
        fn new() -> Self {
            Self {
                metadata: ToolMetadata {
                    side_effect: false,
                    requires_approval: false,
                    cost_hint: None,
                    timeout: None,
                    max_output_tokens: Some(10),
                    source: ToolSource::InProcess,
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl Tool for BigOutputTool {
        fn name(&self) -> &str {
            "big_output"
        }
        fn description(&self) -> &str {
            "returns large output"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &self.metadata
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Immediate(Value::String("x".repeat(1000))))
        }
    }

    #[tokio::test]
    async fn max_output_tokens_truncates_output() {
        let model = Arc::new(AllowedToolsModel {
            target_tool: "big_output".into(),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(BigOutputTool::new())).unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        let completed = events.iter().find_map(|e| match e {
            RuntimeEvent::ToolCallCompleted { output, .. } => Some(output),
            _ => None,
        });
        assert!(completed.is_some(), "should have ToolCallCompleted");
        let output_str = completed.unwrap().as_str().unwrap();
        assert!(output_str.contains("[output truncated]"));
        assert!(output_str.len() < 1000);
    }

    struct MultiToolCallModel {
        call_count: AtomicU32,
    }

    #[async_trait::async_trait]
    impl ModelAdapter for MultiToolCallModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            if count < 5 {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: format!("call_{count}"),
                        name: "echo".into(),
                        input: json!({"n": count}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            }
        }
    }

    #[tokio::test]
    async fn max_tool_calls_boundary_enforced() {
        let mut config = test_config();
        config.budget.max_tool_calls = Some(2);

        let model = Arc::new(MultiToolCallModel {
            call_count: AtomicU32::new(0),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();

        let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        let completed_count = events
            .iter()
            .filter(|e| matches!(e, RuntimeEvent::ToolCallCompleted { .. }))
            .count();
        assert_eq!(completed_count, 2, "should execute exactly max_tool_calls");

        let budget_exceeded = events.iter().any(
            |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool call budget exceeded"),
        );
        assert!(
            budget_exceeded,
            "third tool call should be denied by budget"
        );
    }

    #[test]
    fn truncate_output_string() {
        let value = Value::String("x".repeat(200));
        let result = truncate_output(value, 10);
        let s = result.as_str().unwrap();
        assert!(s.contains("[output truncated]"));
        assert!(s.len() < 200);
    }

    #[test]
    fn truncate_output_json_object() {
        let value = json!({"data": "y".repeat(200)});
        let result = truncate_output(value, 10);
        let s = result.as_str().unwrap();
        assert!(s.contains("[output truncated]"));
    }

    #[test]
    fn truncate_output_small_passes_through() {
        let value = json!("hello");
        let result = truncate_output(value.clone(), 100);
        assert_eq!(result, value);
    }

    #[test]
    fn truncate_output_multibyte_utf8_safe() {
        // 4-byte emoji repeated — the byte boundary must not land in
        // the middle of a character.
        let emoji = "🦀".repeat(20); // 80 bytes
        let value = Value::String(emoji);
        // max_tokens=5 → max_bytes=20, which is 5 crab emojis exactly
        let result = truncate_output(value, 5);
        let s = result.as_str().unwrap();
        assert!(s.contains("[output truncated]"));
        // Must be valid UTF-8 (no panic, no partial chars)
        assert!(s.starts_with("🦀"));

        // Mix of 1-byte and 3-byte chars: "aé" is 3 bytes
        let mixed = "aé".repeat(30); // 90 bytes
        let value2 = Value::String(mixed);
        // max_tokens=2 → max_bytes=8; 'a'=1byte, 'é'=2bytes, "aé"=3bytes
        // 8 bytes fits "aé" twice (6 bytes) + "a" (7) + can't fit "é" (9 > 8)
        // so we get 7 bytes: "aéaéa" — but boundary must be clean
        let result2 = truncate_output(value2, 2);
        let s2 = result2.as_str().unwrap();
        assert!(s2.contains("[output truncated]"));
    }

    #[tokio::test]
    async fn respond_approval_without_pending_returns_error() {
        let model = Arc::new(ToolCallModelAdapter);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        // Drain events — no approval-requiring tools, so no pending
        while rx.recv().await.is_some() {}

        // Sending approval when nothing is pending should fail
        let result = handle.respond_approval(handle.run_id, true).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("no approval pending"),
            "should report no pending approval"
        );

        handle.wait().await;
    }

    #[tokio::test]
    async fn register_skills_scans_and_registers_bundled_tools() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("greet_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: greet_skill
description: A greeting skill
bundled_tools:
  - name: greet
    description: Greets someone
    executable: bash
    script: scripts/greet.sh
    input_schema:
      type: object
      properties:
        name:
          type: string
---

# Greeting Skill
"#,
        )
        .unwrap();
        let script = "#!/bin/sh\nread input\necho '{\"greeting\": \"hello\"}'";
        fs::write(skill_dir.join("scripts/greet.sh"), script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/greet.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let (tx, mut rx) = mpsc::channel(16);
        let mut registry = ToolRegistry::new();

        let result = register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_some()); // read_file tool returned
        assert!(registry.contains("greet"));
        assert!(registry.contains("read_file"));

        // No warnings expected (no scripts/ without capabilities, since we have bundled_tools)
        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        // Skill has scripts/ dir but no capabilities → SkillMissingCapabilities warning
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SkillMissingCapabilities { .. })));
    }

    #[tokio::test]
    async fn register_skills_filters_by_allowed_skills() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        // Create two skills
        for name in &["skill_a", "skill_b"] {
            let skill_dir = tmp.path().join(name);
            fs::create_dir_all(skill_dir.join("scripts")).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!(
                    r#"---
name: {name}
description: {name}
bundled_tools:
  - name: tool_{name}
    description: tool for {name}
    executable: bash
    script: scripts/run.sh
---
"#
                ),
            )
            .unwrap();
            fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    skill_dir.join("scripts/run.sh"),
                    fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            }
        }

        let (tx, _rx) = mpsc::channel(16);
        let mut registry = ToolRegistry::new();
        let allowed = Some(vec!["skill_a".to_string()]);

        register_skills(tmp.path().to_str().unwrap(), &allowed, &mut registry, &tx)
            .await
            .unwrap();

        assert!(registry.contains("tool_skill_a"));
        assert!(!registry.contains("tool_skill_b"));
    }

    #[tokio::test]
    async fn register_skills_duplicate_tool_name_errors() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        // Create two skills with the same bundled tool name
        for name in &["skill_x", "skill_y"] {
            let skill_dir = tmp.path().join(name);
            fs::create_dir_all(skill_dir.join("scripts")).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!(
                    r#"---
name: {name}
description: {name}
bundled_tools:
  - name: same_tool
    description: duplicated tool
    executable: bash
    script: scripts/run.sh
---
"#
                ),
            )
            .unwrap();
            fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    skill_dir.join("scripts/run.sh"),
                    fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            }
        }

        let (tx, _rx) = mpsc::channel(16);
        let mut registry = ToolRegistry::new();

        let result = register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("duplicate tool name"));
    }

    #[tokio::test]
    async fn register_skills_no_warning_when_capabilities_declared() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("cap_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: cap_skill
description: A skill with capabilities
capabilities:
  network: true
bundled_tools:
  - name: cap_tool
    description: tool
    executable: bash
    script: scripts/run.sh
---
"#,
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/run.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let (tx, mut rx) = mpsc::channel(16);
        let mut registry = ToolRegistry::new();

        register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx)
            .await
            .unwrap();

        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        // No SkillMissingCapabilities expected because capabilities are declared
        assert!(!events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SkillMissingCapabilities { .. })));
    }

    #[tokio::test]
    async fn register_skills_emits_missing_capabilities_warning() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("no_cap_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: no_cap_skill
description: A skill WITHOUT capabilities declared
bundled_tools:
  - name: no_cap_tool
    description: tool
    executable: bash
    script: scripts/run.sh
---
"#,
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/run.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let (tx, mut rx) = mpsc::channel(16);
        let mut registry = ToolRegistry::new();

        register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx)
            .await
            .unwrap();

        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        // Should emit SkillMissingCapabilities because no capabilities
        // section is declared in the manifest.
        assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::SkillMissingCapabilities { skill_name, .. } if skill_name == "no_cap_skill")
            ),
            "expected SkillMissingCapabilities for no_cap_skill"
        );
    }

    #[tokio::test]
    async fn skills_dir_config_runs_skill_scan() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("run_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: run_skill
description: A skill
bundled_tools:
  - name: run_tool
    description: runs
    executable: bash
    script: scripts/run.sh
    input_schema:
      type: object
---
"#,
        )
        .unwrap();
        fs::write(
            skill_dir.join("scripts/run.sh"),
            "#!/bin/sh\nread input\necho '{\"result\": \"ok\"}'",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/run.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        // Use a model that calls run_tool then gives final answer
        struct SkillToolModel {
            call_count: std::sync::atomic::AtomicU32,
        }

        #[async_trait::async_trait]
        impl ModelAdapter for SkillToolModel {
            fn provider_name(&self) -> &str {
                "mock"
            }
            fn model_name(&self) -> &str {
                "mock"
            }
            fn capabilities(&self) -> ModelCapabilities {
                ModelCapabilities::default()
            }
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[ToolDef],
                _options: &RequestOptions,
                tx: Option<mpsc::Sender<StreamEvent>>,
            ) -> Result<ModelResponse, ModelError> {
                let count = self
                    .call_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let usage = crate::model::TokenUsage {
                    input_tokens: 5,
                    output_tokens: 5,
                    ..Default::default()
                };
                if let Some(ref tx) = tx {
                    let _ = tx
                        .send(ModelStreamChunk::Done {
                            usage: usage.clone(),
                        })
                        .await;
                }
                if count == 0 {
                    Ok(ModelResponse {
                        content: vec![ContentBlock::ToolUse {
                            id: "call_1".into(),
                            name: "run_tool".into(),
                            input: json!({}),
                        }],
                        usage,
                        stop_reason: StopReason::ToolUse,
                        option_adjustments: vec![],
                    })
                } else {
                    Ok(ModelResponse {
                        content: vec![ContentBlock::Text("done".into())],
                        usage,
                        stop_reason: StopReason::EndTurn,
                        option_adjustments: vec![],
                    })
                }
            }
        }

        let mut config = test_config();
        config.skills_dir = Some(tmp.path().to_str().unwrap().to_string());

        let model = Arc::new(SkillToolModel {
            call_count: std::sync::atomic::AtomicU32::new(0),
        });
        let registry = ToolRegistry::new();

        let (handle, mut rx) = AgentRun::start(config, "test".into(), model, registry);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        handle.wait().await;

        // Should have ToolCallCompleted for run_tool
        let tool_completed = events.iter().any(
            |e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "run_tool"),
        );
        assert!(tool_completed, "run_tool should execute via skill loading");
    }

    // ── Sub-agent approval routing tests ──────────────────────────

    /// Model shared by parent and child.  The parent calls `spawn_sub`
    /// which returns a `__sub_agent_request` with input "child with
    /// approval".  The child sees that text in its first user message
    /// and calls `write_file` (which requires approval).
    struct SubAgentApprovalModel;

    #[async_trait::async_trait]
    impl ModelAdapter for SubAgentApprovalModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let usage = TokenUsage {
                input_tokens: 2,
                output_tokens: 3,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }

            let is_child = messages.iter().any(|m| {
                m.content.iter().any(
                    |c| matches!(c, ContentBlock::Text(t) if t.contains("child with approval")),
                )
            });

            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });

            if has_tool_result {
                let text = if is_child {
                    "child done"
                } else {
                    "parent done"
                };
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text(text.into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            } else if is_child {
                // Child calls write_file which requires approval
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "wf".into(),
                        name: "write_file".into(),
                        input: json!({"path": "test.txt"}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            } else {
                // Parent calls spawn_sub
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "sub".into(),
                        name: "spawn_sub".into(),
                        input: json!({}),
                    }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
        }
    }

    /// Tool that returns a __sub_agent_request for a child that will
    /// call write_file (which requires approval).
    struct SpawnSubTool;

    #[async_trait::async_trait]
    impl Tool for SpawnSubTool {
        fn name(&self) -> &str {
            "spawn_sub"
        }
        fn description(&self) -> &str {
            "spawn a sub-agent"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            }
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Immediate(json!({
                "__sub_agent_request": true,
                "input": "child with approval",
                "config": {
                    "budget": { "max_tokens": 50, "max_tool_calls": 5, "max_duration_secs": 10 }
                }
            })))
        }
    }

    #[tokio::test]
    async fn sub_agent_approval_routed_to_child() {
        let model = Arc::new(SubAgentApprovalModel);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(SpawnSubTool)).unwrap();
        // Register the guarded tool that the child model will call
        registry
            .register(Arc::new(FakeTool::guarded("write_file")))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);

        let mut events = Vec::new();
        let mut child_approval_granted = false;
        let mut child_tool_completed = false;

        loop {
            match rx.recv().await {
                Some(RuntimeEvent::ChildRunEvent {
                    child_run_id,
                    event,
                    ..
                }) => {
                    if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                        // Route approval to the child run
                        handle
                            .respond_approval(child_run_id, true)
                            .await
                            .expect("approval routing to child should succeed");
                    }
                    if matches!(event.as_ref(), RuntimeEvent::ApprovalGranted { .. }) {
                        child_approval_granted = true;
                    }
                    if matches!(event.as_ref(), RuntimeEvent::ToolCallCompleted { .. }) {
                        child_tool_completed = true;
                    }
                    events.push(RuntimeEvent::ChildRunEvent {
                        child_run_id,
                        run_depth: 1,
                        event,
                    });
                }
                Some(event) => events.push(event),
                None => break,
            }
        }
        handle.wait().await;

        assert!(child_approval_granted, "child approval should be granted");
        assert!(
            child_tool_completed,
            "child tool should complete after approval"
        );
        // Should also have SubAgentCompleted
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SubAgentCompleted { .. })));
    }

    #[tokio::test]
    async fn sub_agent_approval_denied_completes_child() {
        let model = Arc::new(SubAgentApprovalModel);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(SpawnSubTool)).unwrap();
        registry
            .register(Arc::new(FakeTool::guarded("write_file")))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);

        let mut events = Vec::new();
        let mut child_approval_denied = false;

        loop {
            match rx.recv().await {
                Some(RuntimeEvent::ChildRunEvent {
                    child_run_id,
                    event,
                    ..
                }) => {
                    if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                        // Deny the approval
                        handle
                            .respond_approval(child_run_id, false)
                            .await
                            .expect("approval routing to child should succeed");
                    }
                    if matches!(event.as_ref(), RuntimeEvent::ApprovalDenied { .. }) {
                        child_approval_denied = true;
                    }
                    events.push(RuntimeEvent::ChildRunEvent {
                        child_run_id,
                        run_depth: 1,
                        event,
                    });
                }
                Some(event) => events.push(event),
                None => break,
            }
        }
        handle.wait().await;

        assert!(
            child_approval_denied,
            "child approval should have been denied"
        );
        // Parent should still complete (SubAgentCompleted or SubAgentFailed)
        let parent_completed = events.iter().any(|e| {
            matches!(e, RuntimeEvent::SubAgentCompleted { .. })
                || matches!(e, RuntimeEvent::SubAgentFailed { .. })
        });
        assert!(
            parent_completed,
            "parent should complete after child denial"
        );
    }

    #[tokio::test]
    async fn respond_approval_unknown_run_id_returns_error() {
        let model = Arc::new(ToolCallModelAdapter);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::echo())).unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

        // Drain events so the run completes
        while rx.recv().await.is_some() {}

        let unknown_id = RunId::new();
        let result = handle.respond_approval(unknown_id, true).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("unknown run_id"),
            "expected error about unknown run_id, got: {err}"
        );

        handle.wait().await;
    }

    #[tokio::test]
    async fn child_run_events_carry_run_depth_and_child_id() {
        let model = Arc::new(SubAgentApprovalModel);
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(SpawnSubTool)).unwrap();
        registry.register(Arc::new(FakeTool::echo())).unwrap();
        // Register write_file so the child doesn't fail on missing tool
        registry
            .register(Arc::new(FakeTool::guarded("write_file")))
            .unwrap();

        let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);
        let parent_run_id = handle.run_id;

        let mut child_run_ids = Vec::new();
        let mut child_depths = Vec::new();

        loop {
            match rx.recv().await {
                Some(RuntimeEvent::ChildRunEvent {
                    child_run_id,
                    run_depth,
                    event,
                    ..
                }) => {
                    if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                        handle.respond_approval(child_run_id, true).await.unwrap();
                    }
                    child_run_ids.push(child_run_id);
                    child_depths.push(run_depth);
                }
                Some(_) => {}
                None => break,
            }
        }
        handle.wait().await;

        assert!(!child_run_ids.is_empty(), "should have child run events");
        // All child events should carry the same child_run_id
        let first_id = child_run_ids[0];
        assert!(
            child_run_ids.iter().all(|id| *id == first_id),
            "all child events should have same child_run_id"
        );
        // run_depth should be 1 (parent is depth 0)
        assert!(
            child_depths.iter().all(|d| *d == 1),
            "child run_depth should be 1"
        );
        // child_run_id should differ from parent run_id
        assert_ne!(
            first_id, parent_run_id,
            "child_run_id must differ from parent run_id"
        );
    }
}
