//! Node.js (napi-rs) bindings for the Orchest agent runtime.
//!
//! This crate only does type conversion and FFI glue between JavaScript and
//! `orchest-runtime`; all business logic lives in core. The public surface
//! is the `Agent` class exposed to Node via napi.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;

use orchest_runtime::bindings::{
    budget_config_from_binding, parse_binding_approval, parse_binding_approval_mode,
    runtime_event_to_wire_value, BindingBudgetConfig, BindingNameStyle,
};
use orchest_runtime::model::{
    CachePolicy, CompatibilityPolicy, ModelSpec, ProviderRuntimeConfig,
    RequestOptions as RustRequestOptions, ThinkingLevel,
};
use orchest_runtime::run::{
    AgentConfig, AgentRun, ModelConfig, RunHandle, RuntimeConfig, SkillsConfig,
};
use orchest_runtime::tool::async_job::{JobHandle, JobStatus, PollFn};
use orchest_runtime::tool::registry::ToolRegistry;
use orchest_runtime::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolExecutionMode, ToolMetadata,
    ToolOutput, ToolSource,
};
use orchest_provider::{create_adapter_from_config, normalize_provider_model};

fn parse_execution_mode_node(
    mode: Option<&str>,
    commit_tool: Option<String>,
    draft_tool: Option<String>,
) -> napi::Result<ToolExecutionMode> {
    match mode {
        None | Some("normal") => Ok(ToolExecutionMode::Normal),
        Some("draft") => {
            let commit_tool = commit_tool.ok_or_else(|| {
                napi::Error::from_reason("executionMode 'draft' requires commitTool")
            })?;
            Ok(ToolExecutionMode::Draft { commit_tool })
        }
        Some("commit") => {
            let draft_tool = draft_tool.ok_or_else(|| {
                napi::Error::from_reason("executionMode 'commit' requires draftTool")
            })?;
            Ok(ToolExecutionMode::Commit { draft_tool })
        }
        Some(other) => Err(napi::Error::from_reason(format!(
            "invalid executionMode '{other}'; expected normal|draft|commit"
        ))),
    }
}

fn execution_mode_from_options(
    options: Option<&serde_json::Value>,
) -> napi::Result<ToolExecutionMode> {
    let mode = options
        .and_then(|o| o.get("executionMode").or_else(|| o.get("execution_mode")))
        .and_then(|v| v.as_str());
    let commit_tool = options
        .and_then(|o| o.get("commitTool").or_else(|| o.get("commit_tool")))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let draft_tool = options
        .and_then(|o| o.get("draftTool").or_else(|| o.get("draft_tool")))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    parse_execution_mode_node(mode, commit_tool, draft_tool)
}

fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("failed to create tokio runtime"))
}

#[napi(object)]
pub struct AgentOptions {
    pub model: String,
    pub system_prompt: String,
    pub skills_dir: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub request_options: Option<RequestOptions>,
    pub budget: Option<BudgetOptions>,
    /// Run-level approval policy: "perTool" | "none" | "all".
    pub approval_mode: Option<String>,
}

#[napi(object)]
pub struct RequestOptions {
    pub thinking: Option<String>,
    pub thinking_budget_tokens: Option<u32>,
    pub include_thinking: Option<bool>,
    pub compatibility_policy: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub cache_policy: Option<String>,
}

#[napi(object)]
pub struct BudgetOptions {
    pub max_tokens: Option<i64>,
    pub max_tool_calls: Option<i32>,
    pub max_duration_secs: Option<i64>,
    pub max_cost_usd: Option<f64>,
}

#[napi(object)]
pub struct ToolRegistration {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub side_effect: Option<bool>,
    /// Approval level: "never" | "whenRisky" | "always".
    pub approval: Option<String>,
    /// Execution mode: "normal" | "draft" | "commit".
    pub execution_mode: Option<String>,
    pub commit_tool: Option<String>,
    pub draft_tool: Option<String>,
}

/// Tool backed by a JavaScript handler function.
/// The handler is called via a ThreadsafeFunction and returns its result
/// through the callback mechanism.
struct JsTool {
    name: String,
    description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    handler: ThreadsafeFunction<Value, ErrorStrategy::Fatal>,
}

const _: () = {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    fn check_threadsafe_function_bounds() {
        assert_send::<ThreadsafeFunction<Value, ErrorStrategy::Fatal>>();
        assert_sync::<ThreadsafeFunction<Value, ErrorStrategy::Fatal>>();
    }

    let _ = check_threadsafe_function_bounds;
};

// Safety: JsTool contains a napi ThreadsafeFunction. The const assertion above
// verifies the concrete ThreadsafeFunction type is Send + Sync for the pinned
// napi-rs version, so forwarding those auto-traits to JsTool is sound.
unsafe impl Send for JsTool {}
unsafe impl Sync for JsTool {}

#[async_trait]
impl Tool for JsTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Value, String>>();
        let tx = std::sync::Mutex::new(Some(tx));

        self.handler.call_with_return_value(
            input,
            ThreadsafeFunctionCallMode::NonBlocking,
            move |result: Value| {
                if let Some(sender) = tx.lock().unwrap().take() {
                    let _ = sender.send(Ok(result));
                }
                Ok(())
            },
        );

        let result = rx.await.map_err(|_| {
            ToolError::fatal("JS handler did not return a value").with_code("JS_HANDLER_ERROR")
        })?;

        let value = result.map_err(|e| ToolError::fatal(e).with_code("JS_HANDLER_ERROR"))?;

        // Check for async job return shape
        if let Some(async_job) = value.get("async_job") {
            if let Some(job_id) = async_job.get("job_id").and_then(|v| v.as_str()) {
                let poll_interval_ms = async_job
                    .get("poll_interval_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1000);
                return Ok(ToolOutput::AsyncJob(JobHandle {
                    job_id: job_id.to_string(),
                    poll: None,
                    poll_interval: Duration::from_millis(poll_interval_ms),
                    timeout: None,
                    webhook: None,
                }));
            }
        }

        Ok(ToolOutput::Immediate(value))
    }
}

/// Async tool backed by a JS initial-handler + a JS poll-handler.
///
/// The initial handler receives the tool input and returns `{ job_id, poll_interval_ms? }`.
/// The poll handler receives `job_id: string` and returns
/// `{ status: "pending"|"completed"|"failed", progress?, message?, result?, error? }`.
struct JsAsyncTool {
    name: String,
    description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    handler: ThreadsafeFunction<Value, ErrorStrategy::Fatal>,
    poll_handler: ThreadsafeFunction<Value, ErrorStrategy::Fatal>,
}

unsafe impl Send for JsAsyncTool {}
unsafe impl Sync for JsAsyncTool {}

#[async_trait]
impl Tool for JsAsyncTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        // Phase 1: call the initial handler to get { job_id, poll_interval_ms? }
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Value, String>>();
        let tx = std::sync::Mutex::new(Some(tx));

        self.handler.call_with_return_value(
            input,
            ThreadsafeFunctionCallMode::NonBlocking,
            move |result: Value| {
                if let Some(sender) = tx.lock().unwrap().take() {
                    let _ = sender.send(Ok(result));
                }
                Ok(())
            },
        );

        let result = rx
            .await
            .map_err(|_| {
                ToolError::fatal("JS async handler channel closed").with_code("CHANNEL_CLOSED")
            })?
            .map_err(|e| ToolError::fatal(e).with_code("JS_HANDLER_ERROR"))?;

        let job_id = result
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::fatal("async tool handler must return { job_id: string }")
                    .with_code("MISSING_JOB_ID")
            })?
            .to_string();

        let poll_interval_ms = result
            .get("poll_interval_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000);

        // Phase 2: wrap the poll handler in a PollFn, passing job_id on each call
        let poll_handler = self.poll_handler.clone();
        let job_id_for_poll = job_id.clone();

        let poll_fn: Arc<PollFn> = Arc::new(move || {
            let ph = poll_handler.clone();
            let jid = job_id_for_poll.clone();
            Box::pin(async move {
                let (tx2, rx2) = tokio::sync::oneshot::channel::<Result<Value, String>>();
                let tx2 = std::sync::Mutex::new(Some(tx2));

                ph.call_with_return_value(
                    Value::String(jid),
                    ThreadsafeFunctionCallMode::NonBlocking,
                    move |result: Value| {
                        if let Some(sender) = tx2.lock().unwrap().take() {
                            let _ = sender.send(Ok(result));
                        }
                        Ok(())
                    },
                );

                let result = rx2
                    .await
                    .map_err(|_| {
                        ToolError::fatal("JS poll handler channel closed")
                            .with_code("CHANNEL_CLOSED")
                    })?
                    .map_err(|e| ToolError::fatal(e).with_code("POLL_HANDLER_ERROR"))?;

                match result.get("status").and_then(|v| v.as_str()) {
                    Some("completed") => Ok(JobStatus::Completed(
                        result.get("result").cloned().unwrap_or(Value::Null),
                    )),
                    Some("failed") => Ok(JobStatus::Failed(
                        result
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("async job failed")
                            .to_string(),
                    )),
                    _ => Ok(JobStatus::Pending {
                        progress: result
                            .get("progress")
                            .and_then(|v| v.as_f64())
                            .map(|v| v as f32),
                        message: result
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    }),
                }
            })
        });

        Ok(ToolOutput::AsyncJob(JobHandle {
            job_id,
            poll: Some(poll_fn),
            poll_interval: Duration::from_millis(poll_interval_ms),
            timeout: None,
            webhook: None,
        }))
    }
}

/// Tool with no handler (schema-only, for backwards compatibility).
struct StaticTool {
    name: String,
    description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
}

#[async_trait]
impl Tool for StaticTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(
        &self,
        _input: Value,
        _ctx: &ToolContext,
    ) -> std::result::Result<ToolOutput, ToolError> {
        Err(ToolError::fatal(format!(
            "tool '{}' has no handler — register with registerToolWithHandler",
            self.name
        ))
        .with_code("NO_HANDLER"))
    }
}

#[napi]
pub struct Agent {
    model: String,
    system_prompt: String,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    max_tokens: Option<u32>,
    request_options: RustRequestOptions,
    skills_dir: Option<String>,
    budget: Option<BudgetOptions>,
    approval_mode: Option<String>,
    tools: Vec<Arc<dyn Tool>>,
    run_handle: Arc<TokioMutex<Option<RunHandle>>>,
}

#[napi]
impl Agent {
    #[napi(constructor)]
    pub fn new(options: AgentOptions) -> napi::Result<Self> {
        Ok(Self {
            model: options.model,
            system_prompt: options.system_prompt,
            api_key: options.api_key,
            api_key_env: options.api_key_env,
            api_url: options.api_url,
            max_tokens: options.max_tokens,
            request_options: options
                .request_options
                .map(rust_request_options_from_js)
                .transpose()
                .map_err(napi::Error::from_reason)?
                .unwrap_or_default(),
            skills_dir: options.skills_dir,
            budget: options.budget,
            approval_mode: options.approval_mode,
            tools: Vec::new(),
            run_handle: Arc::new(TokioMutex::new(None)),
        })
    }

    /// Register a tool with schema only (no handler — tool calls will error).
    /// For tools with handlers, use `registerToolWithHandler`.
    #[napi]
    pub fn register_tool(&mut self, options: ToolRegistration) -> napi::Result<()> {
        let resolved = parse_binding_approval(options.approval.as_deref(), Approval::Never);
        let execution_mode = parse_execution_mode_node(
            options.execution_mode.as_deref(),
            options.commit_tool,
            options.draft_tool,
        )?;
        let tool = StaticTool {
            name: options.name.clone(),
            description: options.description.clone(),
            input_schema: options.input_schema,
            metadata: ToolMetadata {
                side_effect: options.side_effect.unwrap_or(false),
                approval: resolved,
                execution_mode,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
        };

        self.tools.push(Arc::new(tool));
        Ok(())
    }

    /// Register a tool with an executable JavaScript handler function.
    /// The handler receives the parsed input object and should return a
    /// JSON-serializable value. Errors thrown become ToolCallFailed events.
    #[napi(
        ts_args_type = "name: string, description: string, inputSchema: Record<string, unknown>, handler: (input: any) => any, options?: { sideEffect?: boolean, approval?: string, executionMode?: 'normal' | 'draft' | 'commit', commitTool?: string, draftTool?: string }"
    )]
    #[allow(clippy::too_many_arguments)] // justified: NAPI binding mirrors JS API surface
    pub fn register_tool_with_handler(
        &mut self,
        name: String,
        description: String,
        input_schema: serde_json::Value,
        handler: napi::JsFunction,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let approval_str = options
            .as_ref()
            .and_then(|o| o.get("approval"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let side_effect = options
            .as_ref()
            .and_then(|o| o.get("sideEffect"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolved = parse_binding_approval(approval_str.as_deref(), Approval::Never);
        let execution_mode = execution_mode_from_options(options.as_ref())?;

        let tsfn: ThreadsafeFunction<Value, ErrorStrategy::Fatal> = handler
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<Value>| {
                let js_value = ctx.env.to_js_value(&ctx.value)?;
                Ok(vec![js_value])
            })?;

        let tool = JsTool {
            name: name.clone(),
            description,
            input_schema,
            metadata: ToolMetadata {
                side_effect,
                approval: resolved,
                execution_mode,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
            handler: tsfn,
        };

        self.tools.push(Arc::new(tool));
        Ok(())
    }

    /// Register an async tool with a separate poll handler.
    ///
    /// `handler(input)` should return `{ job_id: string, poll_interval_ms?: number }`.
    /// `pollHandler(jobId)` is called on each poll interval and should return
    /// `{ status: "pending"|"completed"|"failed", progress?: number, message?: string, result?: any, error?: string }`.
    #[napi(
        ts_args_type = "name: string, description: string, inputSchema: Record<string, unknown>, handler: (input: any) => { job_id: string; poll_interval_ms?: number }, pollHandler: (jobId: string) => { status: string; progress?: number; message?: string; result?: any; error?: string }, options?: { sideEffect?: boolean; approval?: string, executionMode?: 'normal' | 'draft' | 'commit', commitTool?: string, draftTool?: string }"
    )]
    #[allow(clippy::too_many_arguments)] // justified: napi-rs exposes the JavaScript registration API as positional arguments.
    pub fn register_async_tool_with_handler(
        &mut self,
        name: String,
        description: String,
        input_schema: serde_json::Value,
        handler: napi::JsFunction,
        poll_handler: napi::JsFunction,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let approval_str = options
            .as_ref()
            .and_then(|o| o.get("approval"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let side_effect = options
            .as_ref()
            .and_then(|o| o.get("sideEffect"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolved = parse_binding_approval(approval_str.as_deref(), Approval::Never);
        let execution_mode = execution_mode_from_options(options.as_ref())?;

        let handler_tsfn: ThreadsafeFunction<Value, ErrorStrategy::Fatal> = handler
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<Value>| {
                let js_value = ctx.env.to_js_value(&ctx.value)?;
                Ok(vec![js_value])
            })?;

        let poll_tsfn: ThreadsafeFunction<Value, ErrorStrategy::Fatal> = poll_handler
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<Value>| {
                let js_value = ctx.env.to_js_value(&ctx.value)?;
                Ok(vec![js_value])
            })?;

        let tool = JsAsyncTool {
            name,
            description,
            input_schema,
            metadata: ToolMetadata {
                side_effect,
                approval: resolved,
                execution_mode,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
            handler: handler_tsfn,
            poll_handler: poll_tsfn,
        };

        self.tools.push(Arc::new(tool));
        Ok(())
    }

    #[napi]
    pub async fn run_sync(&self, input: String) -> napi::Result<Vec<serde_json::Value>> {
        let config = self.build_config()?;

        let mut registry = ToolRegistry::new();
        for tool in &self.tools {
            registry
                .register(Arc::clone(tool))
                .map_err(|e| napi::Error::from_reason(format!("{}", e)))?;
        }

        let model: Arc<dyn orchest_runtime::model::ModelAdapter> = Arc::from(
            create_adapter_from_config(self.provider_config())
                .map_err(|e| napi::Error::from_reason(format!("failed to create model: {e}")))?,
        );

        let run_handle_ref = Arc::clone(&self.run_handle);

        let (handle, mut event_rx) = AgentRun::start(config, input, model, registry);

        {
            let mut guard = run_handle_ref.lock().await;
            *guard = Some(handle);
        }

        let mut events = Vec::new();
        while let Some(event) = event_rx.recv().await {
            events.push(event);
        }

        {
            let mut guard = run_handle_ref.lock().await;
            if let Some(h) = guard.take() {
                h.wait().await;
            }
        }

        let mut result = Vec::new();
        for event in &events {
            // Target-language glue remains here: core produces JSON, napi converts it to JS values.
            let value = runtime_event_to_wire_value(event)
                .map_err(|e| napi::Error::from_reason(format!("serialize error: {}", e)))?;
            result.push(value);
        }

        Ok(result)
    }

    #[napi(ts_args_type = "input: string, onEvent: (event: Record<string, unknown>) => void")]
    pub fn run_stream(&self, input: String, on_event: napi::JsFunction) -> napi::Result<()> {
        let config = self.build_config()?;

        let mut registry = ToolRegistry::new();
        for tool in &self.tools {
            registry
                .register(Arc::clone(tool))
                .map_err(|e| napi::Error::from_reason(format!("{}", e)))?;
        }

        let model: Arc<dyn orchest_runtime::model::ModelAdapter> = Arc::from(
            create_adapter_from_config(self.provider_config())
                .map_err(|e| napi::Error::from_reason(format!("failed to create model: {e}")))?,
        );

        let tsfn: ThreadsafeFunction<Value, ErrorStrategy::Fatal> = on_event
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<Value>| {
                let js_value = ctx.env.to_js_value(&ctx.value)?;
                Ok(vec![js_value])
            })?;

        let run_handle_ref = Arc::clone(&self.run_handle);

        let rt = shared_runtime();

        rt.block_on(async {
            let (handle, mut event_rx) = AgentRun::start(config, input, model, registry);

            {
                let mut guard = run_handle_ref.lock().await;
                *guard = Some(handle);
            }

            while let Some(event) = event_rx.recv().await {
                // Target-language glue remains here: core produces JSON, napi converts it to JS values.
                let value = runtime_event_to_wire_value(&event)
                    .map_err(|e| napi::Error::from_reason(format!("serialize error: {}", e)))?;
                let status = tsfn.call(value, ThreadsafeFunctionCallMode::NonBlocking);
                if status != napi::Status::Ok {
                    tracing::warn!("node event callback dropped: {:?}", status);
                }
            }

            {
                let mut guard = run_handle_ref.lock().await;
                if let Some(h) = guard.take() {
                    h.wait().await;
                }
            }

            Ok::<(), napi::Error>(())
        })?;

        Ok(())
    }

    #[napi]
    pub fn respond_approval(&self, run_id: String, approved: bool) -> napi::Result<()> {
        let run_handle_ref = Arc::clone(&self.run_handle);

        let rt = shared_runtime();

        rt.block_on(async {
            let guard = run_handle_ref.lock().await;
            if let Some(ref handle) = *guard {
                let run_id = orchest_runtime::run::RunId(
                    uuid::Uuid::parse_str(&run_id)
                        .map_err(|e| napi::Error::from_reason(format!("invalid run_id: {}", e)))?,
                );
                handle
                    .respond_approval(run_id, approved)
                    .await
                    .map_err(napi::Error::from_reason)?;
                Ok(())
            } else {
                Err(napi::Error::from_reason(
                    "no active run to respond to — call runSync() first",
                ))
            }
        })
    }
}

impl Agent {
    fn provider_config(&self) -> ProviderRuntimeConfig {
        ProviderRuntimeConfig {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            api_key_env: self.api_key_env.clone(),
            api_url: self.api_url.clone(),
            max_tokens: self.max_tokens,
        }
    }

    fn build_config(&self) -> napi::Result<AgentConfig> {
        let budget_config =
            budget_config_from_binding(self.budget.as_ref().map(|b| BindingBudgetConfig {
                max_tokens: b.max_tokens.map(|v| v as u64),
                max_tool_calls: b.max_tool_calls.map(|v| v as u32),
                max_duration_secs: b.max_duration_secs.map(|v| v as u64),
                max_cost_usd: b.max_cost_usd,
            }));

        let normalized = normalize_provider_model(&self.model)
            .map_err(|e| napi::Error::from_reason(format!("invalid model config: {e}")))?;

        Ok(AgentConfig {
            system_prompt: self.system_prompt.clone(),
            model: ModelConfig {
                spec: ModelSpec {
                    provider: normalized.provider.into(),
                    model: normalized.model.into(),
                    api_key_env: self.api_key_env.clone(),
                    api_url: self.api_url.clone(),
                    max_tokens: self.max_tokens,
                    context_window_size: None,
                },
                options: self.request_options.clone(),
            },
            budget: budget_config,
            skills: SkillsConfig {
                dir: self.skills_dir.clone(),
                ..SkillsConfig::default()
            },
            runtime: RuntimeConfig {
                approval_mode: parse_binding_approval_mode(
                    self.approval_mode.as_deref(),
                    BindingNameStyle::Node,
                )
                .map_err(napi::Error::from_reason)?,
                ..RuntimeConfig::default()
            },
            hooks: vec![],
            retry_policy: None,
            handoffs: vec![],
            session_store: None,
            session_id: None,
            supervision_strategy: Default::default(),
        })
    }
}

fn rust_request_options_from_js(options: RequestOptions) -> Result<RustRequestOptions, String> {
    let mut rust = RustRequestOptions::default();
    if let Some(value) = options.thinking {
        rust.thinking = parse_thinking_level(&value)?;
    }
    rust.thinking_budget_tokens = options.thinking_budget_tokens;
    if let Some(value) = options.include_thinking {
        rust.include_thinking = value;
    }
    if let Some(value) = options.compatibility_policy {
        rust.compatibility_policy = parse_compatibility_policy(&value)?;
    }
    rust.max_tokens = options.max_tokens;
    rust.temperature = options.temperature.map(|value| value as f32);
    rust.top_p = options.top_p.map(|value| value as f32);
    if let Some(value) = options.cache_policy {
        rust.cache_policy = parse_cache_policy(&value)?;
    }
    Ok(rust)
}

fn parse_thinking_level(value: &str) -> Result<ThinkingLevel, String> {
    match value {
        "off" => Ok(ThinkingLevel::Off),
        "minimal" => Ok(ThinkingLevel::Minimal),
        "low" => Ok(ThinkingLevel::Low),
        "medium" => Ok(ThinkingLevel::Medium),
        "high" => Ok(ThinkingLevel::High),
        "xhigh" => Ok(ThinkingLevel::XHigh),
        "max" => Ok(ThinkingLevel::Max),
        _ => Err(format!(
            "invalid thinking value '{value}'; expected off|minimal|low|medium|high|xhigh|max"
        )),
    }
}

fn parse_compatibility_policy(value: &str) -> Result<CompatibilityPolicy, String> {
    match value {
        "coerce" => Ok(CompatibilityPolicy::Coerce),
        "strict" => Ok(CompatibilityPolicy::Strict),
        _ => Err(format!(
            "invalid compatibilityPolicy value '{value}'; expected coerce|strict"
        )),
    }
}

fn parse_cache_policy(value: &str) -> Result<CachePolicy, String> {
    match value {
        "none" => Ok(CachePolicy::None),
        "auto" => Ok(CachePolicy::Auto),
        "long" => Ok(CachePolicy::Long),
        _ => Err(format!(
            "invalid cachePolicy value '{value}'; expected none|auto|long"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_runtime::bindings::{runtime_event_value_to_wire_value, to_snake_case};

    #[test]
    fn runtime_event_type_uses_snake_case_wire_format() {
        let event = serde_json::json!({
            "ModelStreamChunk": {
                "delta": { "Text": { "delta": "hello" } }
            }
        });

        let converted = runtime_event_value_to_wire_value(event);

        assert_eq!(converted["type"], "model_stream_chunk");
        assert!(converted.get("delta").is_some());
    }

    #[test]
    fn runtime_event_metadata_uses_snake_case_wire_fields() {
        let event = serde_json::json!({
            "RunAborted": {
                "reason": null
            }
        });

        let converted = runtime_event_value_to_wire_value(event);

        assert_eq!(converted["type"], "run_aborted");
        assert_eq!(converted["run_depth"], 0);
        assert!(converted["child_run_id"].is_null());
        assert!(converted.get(format!("{}{}", "run", "Depth")).is_none());
        assert!(converted.get(format!("{}{}", "child", "RunId")).is_none());
    }

    #[test]
    fn snake_case_conversion_handles_runtime_event_names() {
        assert_eq!(to_snake_case("RunStarted"), "run_started");
        assert_eq!(to_snake_case("ApprovalDenied"), "approval_denied");
        assert_eq!(to_snake_case("AsyncToolProgress"), "async_tool_progress");
    }

    #[test]
    fn node_request_options_maps_to_rust_request_options() {
        let options = rust_request_options_from_js(RequestOptions {
            thinking: Some("xhigh".into()),
            thinking_budget_tokens: Some(1024),
            include_thinking: Some(false),
            compatibility_policy: Some("strict".into()),
            max_tokens: Some(2048),
            temperature: Some(0.3),
            top_p: Some(0.7),
            cache_policy: Some("long".into()),
        })
        .expect("request options should parse");

        assert_eq!(options.thinking, ThinkingLevel::XHigh);
        assert_eq!(options.thinking_budget_tokens, Some(1024));
        assert!(!options.include_thinking);
        assert_eq!(options.compatibility_policy, CompatibilityPolicy::Strict);
        assert_eq!(options.max_tokens, Some(2048));
        assert_eq!(options.cache_policy, CachePolicy::Long);
    }

    #[test]
    fn node_request_options_rejects_invalid_enum() {
        let err = rust_request_options_from_js(RequestOptions {
            thinking: Some("very".into()),
            thinking_budget_tokens: None,
            include_thinking: None,
            compatibility_policy: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            cache_policy: None,
        })
        .expect_err("invalid enum should fail");
        assert!(err.contains("off|minimal|low|medium|high|xhigh|max"));
    }

    #[test]
    fn node_execution_mode_defaults_to_normal() {
        let mode = parse_execution_mode_node(None, None, None).expect("normal mode should parse");

        assert_eq!(mode, ToolExecutionMode::Normal);
    }

    #[test]
    fn node_execution_mode_parses_commit() {
        let mode = parse_execution_mode_node(Some("commit"), None, Some("draft_write".into()))
            .expect("commit mode should parse");

        assert_eq!(
            mode,
            ToolExecutionMode::Commit {
                draft_tool: "draft_write".into()
            }
        );
    }

    #[test]
    fn node_execution_mode_from_handler_options_accepts_camel_case() {
        let options = serde_json::json!({
            "executionMode": "draft",
            "commitTool": "commit_write"
        });

        let mode = execution_mode_from_options(Some(&options)).expect("options should parse");

        assert_eq!(
            mode,
            ToolExecutionMode::Draft {
                commit_tool: "commit_write".into()
            }
        );
    }

    #[test]
    fn node_agent_provider_config_preserves_canonical_model() {
        let agent = Agent::new(AgentOptions {
            model: "openrouter/anthropic/claude-sonnet-4".into(),
            system_prompt: "test".into(),
            skills_dir: None,
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: Some("http://localhost".into()),
            max_tokens: Some(123),
            request_options: None,
            budget: None,
            approval_mode: None,
        })
        .expect("agent should construct");
        let config = agent.provider_config();
        assert_eq!(config.model, "openrouter/anthropic/claude-sonnet-4");
        assert_eq!(config.max_tokens, Some(123));
    }

    #[test]
    fn node_agent_config_normalizes_legacy_shorthand() {
        let agent = Agent::new(AgentOptions {
            model: "claude-sonnet-4".into(),
            system_prompt: "test".into(),
            skills_dir: None,
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            max_tokens: None,
            request_options: None,
            budget: None,
            approval_mode: None,
        })
        .expect("agent should construct");
        let config = agent.build_config().expect("config should build");
        assert_eq!(config.model.spec.provider, "anthropic");
        assert_eq!(config.model.spec.model, "claude-sonnet-4");
    }
}
