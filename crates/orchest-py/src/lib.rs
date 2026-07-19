//! Python (PyO3) bindings for the Orchest agent runtime.
//!
//! This crate only does type conversion and FFI glue between Python and
//! `orchest`; all business logic lives in core. The public surface
//! is the `Agent` class exposed to Python via PyO3.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;

use orchest::bindings::{
    budget_config_from_binding, messages_from_wire_values, parse_binding_approval,
    parse_binding_approval_mode, runtime_event_to_wire_value, BindingBudgetConfig,
    BindingNameStyle,
};
use orchest::events::RuntimeEvent;
use orchest::model::{
    CachePolicy, CompatibilityPolicy, ModelSpec, ProviderRuntimeConfig, RequestOptions,
    ThinkingLevel,
};
use orchest::run::{
    AgentConfig, AgentRun, ApprovalMode, ModelConfig, RetryPolicy, RunHandle, RunInput,
    RuntimeConfig, SkillsConfig,
};
use orchest::tool::async_job::{JobHandle, JobStatus};
use orchest::tool::builtin::WriteFileTool;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolExecutionMode, ToolMetadata,
    ToolOutput, ToolSource,
};
use orchest_provider::{create_adapter_from_config, normalize_provider_model};

fn parse_execution_mode(
    mode: Option<&str>,
    commit_tool: Option<String>,
    draft_tool: Option<String>,
) -> PyResult<ToolExecutionMode> {
    match mode {
        None | Some("normal") => Ok(ToolExecutionMode::Normal),
        Some("draft") => {
            let commit_tool = commit_tool.ok_or_else(|| {
                PyRuntimeError::new_err("execution_mode='draft' requires commit_tool")
            })?;
            Ok(ToolExecutionMode::Draft { commit_tool })
        }
        Some("commit") => {
            let draft_tool = draft_tool.ok_or_else(|| {
                PyRuntimeError::new_err("execution_mode='commit' requires draft_tool")
            })?;
            Ok(ToolExecutionMode::Commit { draft_tool })
        }
        Some(other) => Err(PyRuntimeError::new_err(format!(
            "invalid execution_mode '{other}'; expected normal|draft|commit"
        ))),
    }
}

fn await_coroutine(py: Python<'_>, coro: Py<PyAny>) -> PyResult<Py<PyAny>> {
    run_coroutine_on_thread(py, coro)
}

fn run_coroutine_on_thread(py: Python<'_>, coro: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let coro_clone = coro.clone_ref(py);

    std::thread::spawn(move || {
        let result = Python::attach(|py| {
            py.import("asyncio")
                .and_then(|asyncio| asyncio.call_method1("run", (coro_clone.bind(py),)))
                .map(|value| value.unbind())
        });
        let _ = tx.send(result);
    });

    py.detach(move || {
        rx.recv()
            .map_err(|_| PyRuntimeError::new_err("async coroutine thread panicked"))
    })?
}

#[pyclass]
struct Agent {
    model: String,
    system_prompt: String,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    max_tokens: Option<u32>,
    request_options: RequestOptions,
    skills_dir: Option<String>,
    budget: Option<PyBudget>,
    approval_mode: Option<String>,
    retry: Option<bool>,
    tools: Vec<PyToolDef>,
    native_tools: Vec<Arc<dyn Tool>>,
    run_handle: Arc<TokioMutex<Option<RunHandle>>>,
}

#[derive(Clone)]
struct PyBudget {
    max_tokens: Option<u64>,
    max_tool_calls: Option<u32>,
    max_duration_secs: Option<u64>,
    max_cost_usd: Option<f64>,
}

#[derive(Clone)]
struct PyToolDef {
    name: String,
    description: String,
    input_schema: Value,
    approval: Approval,
    side_effect: bool,
    execution_mode: ToolExecutionMode,
    callback: Py<PyAny>,
}

struct PyTool {
    def: PyToolDef,
    metadata: ToolMetadata,
}

#[async_trait]
impl Tool for PyTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.def.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let callback = &self.def.callback;
        let input_str = serde_json::to_string(&input)
            .map_err(|e| ToolError::fatal(format!("failed to serialize input: {}", e)))?;

        let result = Python::attach(|py| -> PyResult<Py<PyAny>> {
            let json_mod = py.import("json")?;
            let input_py = json_mod.call_method1("loads", (&input_str,))?;

            let raw_result = if let Ok(kwargs) = input_py.cast::<PyDict>() {
                callback.call(py, (), Some(kwargs))?
            } else {
                callback.call1(py, (input_py,))?
            };

            // If the result is a coroutine, run it with asyncio
            let inspect = py.import("inspect")?;
            let is_coro: bool = inspect
                .call_method1("iscoroutine", (raw_result.bind(py),))?
                .extract()?;
            if is_coro {
                await_coroutine(py, raw_result)
            } else {
                Ok(raw_result)
            }
        })
        .map_err(|e| ToolError::fatal(format!("Python tool error: {}", e)))?;

        if let Some(job_handle) = py_async_job_handle(&result)? {
            return Ok(ToolOutput::AsyncJob(job_handle));
        }

        let result_str = Python::attach(|py| -> PyResult<String> {
            let json_mod = py.import("json")?;
            json_mod
                .call_method1("dumps", (result.bind(py),))?
                .extract()
        })
        .map_err(|e| ToolError::fatal(format!("failed to serialize Python return value: {}", e)))?;

        let value: Value = serde_json::from_str(&result_str)
            .map_err(|e| ToolError::fatal(format!("failed to parse Python return value: {}", e)))?;

        Ok(ToolOutput::Immediate(value))
    }
}

fn py_async_job_handle(result: &Py<PyAny>) -> Result<Option<JobHandle>, ToolError> {
    Python::attach(|py| -> PyResult<Option<JobHandle>> {
        let dict = match result.bind(py).cast::<PyDict>() {
            Ok(dict) => dict,
            Err(_) => return Ok(None),
        };
        let Some(async_job) = dict.get_item("async_job")? else {
            return Ok(None);
        };
        let async_job = async_job.cast::<PyDict>()?;
        let job_id: String = async_job
            .get_item("job_id")?
            .ok_or_else(|| PyRuntimeError::new_err("async_job missing job_id"))?
            .extract()?;
        let poll_interval_ms = async_job
            .get_item("poll_interval_ms")?
            .and_then(|v| v.extract::<u64>().ok())
            .unwrap_or(1000);
        let poll: Py<PyAny> = async_job
            .get_item("poll")?
            .ok_or_else(|| PyRuntimeError::new_err("async_job missing poll"))?
            .extract()?;

        let poll_fn = move || {
            let poll = Python::attach(|py| poll.clone_ref(py));
            Box::pin(async move {
                Python::attach(|py| -> Result<JobStatus, ToolError> {
                    let raw_value = poll.call0(py).map_err(|e| {
                        ToolError::fatal(format!("Python async job poll error: {}", e))
                    })?;

                    // If the poll result is a coroutine, await it
                    let inspect = py.import("inspect").map_err(|e| {
                        ToolError::fatal(format!("failed to import inspect: {}", e))
                    })?;
                    let is_coro: bool = inspect
                        .call_method1("iscoroutine", (raw_value.bind(py),))
                        .and_then(|v| v.extract())
                        .unwrap_or(false);

                    let value = if is_coro {
                        await_coroutine(py, raw_value).map_err(|e| {
                            ToolError::fatal(format!("failed to await async poll: {}", e))
                        })?
                    } else {
                        raw_value
                    };

                    let json_mod = py
                        .import("json")
                        .map_err(|e| ToolError::fatal(format!("failed to import json: {}", e)))?;
                    let json_str: String = json_mod
                        .call_method1("dumps", (value.bind(py),))
                        .and_then(|v| v.extract())
                        .map_err(|e| {
                            ToolError::fatal(format!("failed to serialize poll result: {}", e))
                        })?;
                    let parsed: Value = serde_json::from_str(&json_str).map_err(|e| {
                        ToolError::fatal(format!("failed to parse poll result: {}", e))
                    })?;

                    match parsed.get("status").and_then(|v| v.as_str()) {
                        Some("completed") => Ok(JobStatus::Completed(
                            parsed.get("result").cloned().unwrap_or(Value::Null),
                        )),
                        Some("failed") => Ok(JobStatus::Failed(
                            parsed
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown error")
                                .to_string(),
                        )),
                        _ => Ok(JobStatus::Pending {
                            progress: parsed
                                .get("progress")
                                .and_then(|v| v.as_f64())
                                .map(|v| v as f32),
                            message: parsed
                                .get("message")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        }),
                    }
                })
            })
                as std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<JobStatus, ToolError>> + Send>,
                >
        };

        Ok(Some(JobHandle {
            job_id,
            poll: Some(Arc::new(poll_fn)),
            poll_interval: Duration::from_millis(poll_interval_ms),
            timeout: None,
            webhook: None,
        }))
    })
    .map_err(|e| ToolError::fatal(format!("invalid Python async job return value: {}", e)))
}

fn infer_schema_from_hints(py: Python<'_>, func: &Py<PyAny>) -> PyResult<Value> {
    let inspect = py.import("inspect")?;
    let sig = inspect.call_method1("signature", (func,))?;
    let params = sig.getattr("parameters")?;
    let items = params.call_method0("items")?;

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for item in items.try_iter()? {
        let item = item?;
        let tuple = item.cast::<pyo3::types::PyTuple>()?;
        let name: String = tuple.get_item(0)?.extract()?;
        let param = tuple.get_item(1)?;

        let annotation = param.getattr("annotation")?;
        let empty = inspect.getattr("Parameter")?.getattr("empty")?;

        if annotation.is(&empty) {
            properties.insert(name.clone(), Value::Object(serde_json::Map::new()));
        } else {
            let builtins = py.import("builtins")?;
            let type_schema = if annotation.is(builtins.getattr("str")?) {
                serde_json::json!({"type": "string"})
            } else if annotation.is(builtins.getattr("int")?) {
                serde_json::json!({"type": "integer"})
            } else if annotation.is(builtins.getattr("float")?) {
                serde_json::json!({"type": "number"})
            } else if annotation.is(builtins.getattr("bool")?) {
                serde_json::json!({"type": "boolean"})
            } else {
                Value::Object(serde_json::Map::new())
            };
            properties.insert(name.clone(), type_schema);
        }

        let default = param.getattr("default")?;
        let param_empty = inspect.getattr("Parameter")?.getattr("empty")?;
        if default.is(&param_empty) {
            required.push(Value::String(name));
        }
    }

    Ok(serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required
    }))
}

/// Converts a Python list of message dicts (the serde JSON shape of the core
/// `Message` type) into core messages for `AgentRun::start_with_messages`.
/// Conversion goes through the stdlib `json` module, mirroring
/// `runtime_event_to_dict` in the opposite direction.
fn py_messages_to_core(
    py: Python<'_>,
    messages: Option<Vec<Py<PyAny>>>,
) -> PyResult<Vec<orchest::model::Message>> {
    let Some(messages) = messages else {
        return Ok(vec![]);
    };
    let json_mod = py.import("json")?;
    let mut values = Vec::with_capacity(messages.len());
    for message in &messages {
        let json_str: String = json_mod
            .call_method1("dumps", (message.bind(py),))?
            .extract()?;
        values.push(
            serde_json::from_str(&json_str)
                .map_err(|e| PyRuntimeError::new_err(format!("failed to parse message: {e}")))?,
        );
    }
    messages_from_wire_values(values)
        .map_err(|e| PyRuntimeError::new_err(format!("invalid `messages` entry: {e}")))
}

fn runtime_event_to_dict(py: Python<'_>, event: &RuntimeEvent) -> PyResult<Py<PyDict>> {
    // Target-language glue remains here: core produces JSON, PyO3 converts it to a Python dict.
    let event_obj = runtime_event_to_wire_value(event)
        .map_err(|e| PyRuntimeError::new_err(format!("failed to serialize event: {}", e)))?;
    let json_str = serde_json::to_string(&event_obj)
        .map_err(|e| PyRuntimeError::new_err(format!("failed to serialize event dict: {}", e)))?;

    let json_mod = py.import("json")?;
    let dict = json_mod.call_method1("loads", (&json_str,))?;
    let dict: Py<PyDict> = dict.extract()?;

    Ok(dict)
}

fn parse_request_options_value(value: Option<Value>) -> Result<RequestOptions, String> {
    let Some(Value::Object(map)) = value else {
        return Ok(RequestOptions::default());
    };
    let mut options = RequestOptions::default();
    if let Some(value) = map.get("thinking").and_then(Value::as_str) {
        options.thinking = parse_thinking_level(value)?;
    }
    if let Some(value) = map.get("thinking_budget_tokens").and_then(Value::as_u64) {
        options.thinking_budget_tokens = Some(value as u32);
    }
    if let Some(value) = map.get("include_thinking").and_then(Value::as_bool) {
        options.include_thinking = value;
    }
    if let Some(value) = map.get("compatibility_policy").and_then(Value::as_str) {
        options.compatibility_policy = parse_compatibility_policy(value)?;
    }
    if let Some(value) = map.get("max_tokens").and_then(Value::as_u64) {
        options.max_tokens = Some(value as u32);
    }
    if let Some(value) = map.get("temperature").and_then(Value::as_f64) {
        options.temperature = Some(value as f32);
    }
    if let Some(value) = map.get("top_p").and_then(Value::as_f64) {
        options.top_p = Some(value as f32);
    }
    if let Some(value) = map.get("cache_policy").and_then(Value::as_str) {
        options.cache_policy = parse_cache_policy(value)?;
    }
    Ok(options)
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

fn parse_approval_mode(value: Option<&str>) -> PyResult<ApprovalMode> {
    parse_binding_approval_mode(value, BindingNameStyle::Python).map_err(PyRuntimeError::new_err)
}

fn parse_compatibility_policy(value: &str) -> Result<CompatibilityPolicy, String> {
    match value {
        "coerce" => Ok(CompatibilityPolicy::Coerce),
        "strict" => Ok(CompatibilityPolicy::Strict),
        _ => Err(format!(
            "invalid compatibility_policy value '{value}'; expected coerce|strict"
        )),
    }
}

fn parse_cache_policy(value: &str) -> Result<CachePolicy, String> {
    match value {
        "none" => Ok(CachePolicy::None),
        "auto" => Ok(CachePolicy::Auto),
        "long" => Ok(CachePolicy::Long),
        _ => Err(format!(
            "invalid cache_policy value '{value}'; expected none|auto|long"
        )),
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

    fn build_config(&self) -> PyResult<AgentConfig> {
        let budget_config =
            budget_config_from_binding(self.budget.as_ref().map(|b| BindingBudgetConfig {
                max_tokens: b.max_tokens,
                max_tool_calls: b.max_tool_calls,
                max_duration_secs: b.max_duration_secs,
                max_cost_usd: b.max_cost_usd,
            }));

        let normalized = normalize_provider_model(&self.model)
            .map_err(|e| PyRuntimeError::new_err(format!("invalid model config: {e}")))?;

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
                approval_mode: parse_approval_mode(self.approval_mode.as_deref())?,
                ..RuntimeConfig::default()
            },
            hooks: vec![],
            // `retry=True` opts into the recommended policy (429/5xx/timeout/
            // stream-interrupt); default stays off (None) as before.
            retry_policy: if self.retry.unwrap_or(false) {
                Some(RetryPolicy::recommended())
            } else {
                None
            },
            handoffs: vec![],
            session_store: None,
            session_id: None,
            supervision_strategy: Default::default(),
        })
    }

    fn build_registry(&self) -> Result<ToolRegistry, PyErr> {
        let mut registry = ToolRegistry::new();
        for tool in &self.native_tools {
            registry
                .register(Arc::clone(tool))
                .map_err(|e| PyRuntimeError::new_err(format!("failed to register tool: {}", e)))?;
        }
        for tool_def in &self.tools {
            let tool = PyTool {
                def: tool_def.clone(),
                metadata: ToolMetadata {
                    side_effect: tool_def.side_effect,
                    approval: tool_def.approval,
                    execution_mode: tool_def.execution_mode.clone(),
                    source: ToolSource::InProcess,
                    ..ToolMetadata::default()
                },
            };
            registry
                .register(Arc::new(tool))
                .map_err(|e| PyRuntimeError::new_err(format!("failed to register tool: {}", e)))?;
        }
        Ok(registry)
    }

    fn build_model(&self) -> Result<Arc<dyn orchest::model::ModelAdapter>, PyErr> {
        let adapter = create_adapter_from_config(self.provider_config())
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create model: {}", e)))?;
        Ok(Arc::from(adapter))
    }
}

#[pymethods]
impl Agent {
    #[new]
    #[pyo3(signature = (model, system_prompt, skills_dir=None, budget=None, api_url=None, api_key=None, api_key_env=None, max_tokens=None, request_options=None, approval_mode=None, retry=None))]
    #[allow(clippy::too_many_arguments)] // justified: pyo3 constructor maps Python kwargs 1:1
    fn new(
        model: String,
        system_prompt: String,
        skills_dir: Option<String>,
        budget: Option<Bound<'_, PyDict>>,
        api_url: Option<String>,
        api_key: Option<String>,
        api_key_env: Option<String>,
        max_tokens: Option<u32>,
        request_options: Option<Bound<'_, PyDict>>,
        approval_mode: Option<String>,
        retry: Option<bool>,
    ) -> PyResult<Self> {
        let py_budget = if let Some(b) = budget {
            Some(PyBudget {
                max_tokens: b.get_item("max_tokens")?.and_then(|v| v.extract().ok()),
                max_tool_calls: b.get_item("max_tool_calls")?.and_then(|v| v.extract().ok()),
                max_duration_secs: b
                    .get_item("max_duration_secs")?
                    .and_then(|v| v.extract().ok()),
                max_cost_usd: b.get_item("max_cost_usd")?.and_then(|v| v.extract().ok()),
            })
        } else {
            None
        };

        let request_options = if let Some(options) = request_options {
            let json_mod = options.py().import("json")?;
            let json_str: String = json_mod.call_method1("dumps", (&options,))?.extract()?;
            let value: Value = serde_json::from_str(&json_str).map_err(|e| {
                PyRuntimeError::new_err(format!("failed to parse request_options: {e}"))
            })?;
            parse_request_options_value(Some(value)).map_err(PyRuntimeError::new_err)?
        } else {
            RequestOptions::default()
        };

        Ok(Self {
            model,
            system_prompt,
            api_key,
            api_key_env,
            api_url,
            max_tokens,
            request_options,
            skills_dir,
            budget: py_budget,
            approval_mode,
            retry,
            tools: Vec::new(),
            native_tools: Vec::new(),
            run_handle: Arc::new(TokioMutex::new(None)),
        })
    }

    fn set_api_url(&mut self, api_url: Option<String>) {
        self.api_url = api_url;
    }

    /// Register a tool. Supports `@agent.tool` (bare decorator).
    #[pyo3(signature = (func=None, side_effect=false, approval=None, execution_mode=None, commit_tool=None, draft_tool=None))]
    #[allow(clippy::too_many_arguments)] // justified: Python decorator metadata surface
    fn tool(
        &mut self,
        py: Python<'_>,
        func: Option<Py<PyAny>>,
        side_effect: bool,
        approval: Option<String>,
        execution_mode: Option<String>,
        commit_tool: Option<String>,
        draft_tool: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        if let Some(func) = func {
            let name: String = func.getattr(py, "__name__")?.extract(py)?;
            let description: String = func
                .getattr(py, "__doc__")
                .and_then(|d| d.extract(py))
                .unwrap_or_else(|_| format!("Tool: {}", name));
            let input_schema = infer_schema_from_hints(py, &func)?;
            let resolved = parse_binding_approval(approval.as_deref(), Approval::Never);
            let execution_mode =
                parse_execution_mode(execution_mode.as_deref(), commit_tool, draft_tool)?;

            self.tools.push(PyToolDef {
                name,
                description,
                input_schema,
                approval: resolved,
                side_effect,
                execution_mode,
                callback: func.clone_ref(py),
            });

            Ok(func)
        } else {
            Err(PyRuntimeError::new_err(
                "Use @agent.tool directly for decorator syntax. For metadata, use agent.register_tool(func, approval=\"always\", side_effect=True).",
            ))
        }
    }

    /// Explicitly register a tool with metadata options.
    #[pyo3(signature = (func, side_effect=false, approval=None, execution_mode=None, commit_tool=None, draft_tool=None))]
    #[allow(clippy::too_many_arguments)] // justified: Python tool metadata surface
    fn register_tool(
        &mut self,
        py: Python<'_>,
        func: Py<PyAny>,
        side_effect: bool,
        approval: Option<String>,
        execution_mode: Option<String>,
        commit_tool: Option<String>,
        draft_tool: Option<String>,
    ) -> PyResult<()> {
        let name: String = func.getattr(py, "__name__")?.extract(py)?;
        let description: String = func
            .getattr(py, "__doc__")
            .and_then(|d| d.extract(py))
            .unwrap_or_else(|_| format!("Tool: {}", name));
        let input_schema = infer_schema_from_hints(py, &func)?;
        let resolved = parse_binding_approval(approval.as_deref(), Approval::Never);
        let execution_mode =
            parse_execution_mode(execution_mode.as_deref(), commit_tool, draft_tool)?;

        self.tools.push(PyToolDef {
            name,
            description,
            input_schema,
            approval: resolved,
            side_effect,
            execution_mode,
            callback: func,
        });

        Ok(())
    }

    #[pyo3(signature = (name, description, agent, input_key=None))]
    fn register_agent_tool(
        &mut self,
        name: String,
        description: String,
        agent: &Agent,
        input_key: Option<String>,
    ) -> PyResult<()> {
        let input_key = input_key.unwrap_or_else(|| "question".to_string());
        let child_config = agent.build_config()?;
        let child_registry = agent.build_registry()?;
        let child_model = agent.build_model()?;
        let _input_schema = serde_json::json!({
            "type": "object",
            "properties": {
                input_key.clone(): {
                    "type": "string",
                    "description": "Task or question to delegate to the child agent"
                }
            },
            "required": [input_key.clone()]
        });
        let mapper_key = input_key.clone();
        let input_mapper = Arc::new(move |value: Value| {
            value
                .get(&mapper_key)
                .and_then(Value::as_str)
                .map(String::from)
                .ok_or_else(|| {
                    ToolError::fatal(format!("missing required parameter '{mapper_key}'"))
                        .with_code("MISSING_PARAM")
                })
        });
        let output_mapper = Arc::new(|details: Value| {
            details
                .get("output")
                .cloned()
                .unwrap_or_else(|| details.clone())
        });

        self.native_tools.push(
            child_config
                .as_tool(&name, &description)
                .model(child_model)
                .registry(child_registry)
                .input_mapper(move |v| input_mapper(v))
                .output_extractor(move |v| output_mapper(v))
                .build()
                .map_err(|e| {
                    PyRuntimeError::new_err(format!("failed to build sub-agent tool: {}", e))
                })?,
        );

        Ok(())
    }

    #[pyo3(signature = (approval=None))]
    fn register_write_file_tool(&mut self, approval: Option<String>) -> PyResult<()> {
        let resolved = parse_binding_approval(approval.as_deref(), Approval::Always);
        self.native_tools
            .push(Arc::new(WriteFileTool::new_with_approval(resolved)));
        Ok(())
    }

    /// Synchronous run: collects all events and returns as a list.
    /// Compatibility helper for simple use cases.
    ///
    /// `messages` (optional) is the prior conversation as a list of message
    /// dicts in the core serde shape, e.g.
    /// `{"role": "user", "content": [{"Text": "..."}]}`; when given, the run
    /// starts via `AgentRun::start_with_messages` with that history.
    #[pyo3(signature = (input, messages=None))]
    fn run_sync<'py>(
        &self,
        py: Python<'py>,
        input: String,
        messages: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<Bound<'py, pyo3::types::PyList>> {
        let config = self.build_config()?;
        let registry = self.build_registry()?;
        let model_adapter = self.build_model()?;
        let initial_messages = py_messages_to_core(py, messages)?;
        let run_handle_ref = Arc::clone(&self.run_handle);

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create runtime: {}", e)))?;

        let events = py.detach(|| {
            rt.block_on(async {
                let (handle, mut event_rx) = AgentRun::start_with_messages(
                    config,
                    initial_messages,
                    RunInput::text(input),
                    model_adapter,
                    registry,
                );

                // Store the handle for respond_approval
                {
                    let mut guard = run_handle_ref.lock().await;
                    *guard = Some(handle);
                }

                let mut events = Vec::new();
                while let Some(event) = event_rx.recv().await {
                    events.push(event);
                }

                // Wait for run to finish and clear handle
                {
                    let mut guard = run_handle_ref.lock().await;
                    if let Some(h) = guard.take() {
                        h.wait().await;
                    }
                }

                events
            })
        });

        let py_list = pyo3::types::PyList::empty(py);
        for event in &events {
            let dict = runtime_event_to_dict(py, event)?;
            py_list.append(dict)?;
        }

        Ok(py_list)
    }

    /// Async-compatible run: returns a list of events (async iteration
    /// over a channel requires a Python async generator, which is complex
    /// in pyo3. This method releases the GIL during execution.)
    ///
    /// `messages` (optional) is the prior conversation, as in `run_sync`.
    #[pyo3(signature = (input, messages=None))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        input: String,
        messages: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<Bound<'py, pyo3::types::PyList>> {
        // For now, run and run_sync have the same implementation.
        // True async iteration would require pyo3-asyncio integration
        // which adds significant complexity. The key improvement is that
        // we release the GIL during execution via py.detach().
        self.run_sync(py, input, messages)
    }

    /// Streaming run: calls `on_event(dict)` for each event as it arrives.
    ///
    /// `messages` (optional) is the prior conversation, as in `run_sync`.
    ///
    /// Runs the agent in a background thread and processes events on the
    /// Python side with periodic signal checks so Ctrl+C works.
    #[pyo3(signature = (input, on_event, messages=None))]
    fn run_stream<'py>(
        &self,
        py: Python<'py>,
        input: String,
        on_event: Py<PyAny>,
        messages: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<()> {
        let config = self.build_config()?;
        let registry = self.build_registry()?;
        let model_adapter = self.build_model()?;
        let initial_messages = py_messages_to_core(py, messages)?;
        let run_handle_ref = Arc::clone(&self.run_handle);

        let (tx, rx) = std::sync::mpsc::channel::<RuntimeEvent>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(async move {
                let (handle, mut event_rx) = AgentRun::start_with_messages(
                    config,
                    initial_messages,
                    RunInput::text(input),
                    model_adapter,
                    registry,
                );
                {
                    let mut guard = run_handle_ref.lock().await;
                    *guard = Some(handle);
                }
                while let Some(event) = event_rx.recv().await {
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                // channel drop signals Python side we are done
            });
        });

        loop {
            py.check_signals()?;
            match rx.try_recv() {
                Ok(event) => {
                    let dict = runtime_event_to_dict(py, &event)?;
                    on_event.call1(py, (dict,))?;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Release GIL while sleeping so Python signal handlers can run.
                    py.detach(|| std::thread::sleep(Duration::from_millis(10)));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }

        Ok(())
    }

    /// Respond to an approval request for an active run.
    fn respond_approval(&self, run_id_str: String, approved: bool) -> PyResult<()> {
        let run_handle_ref = Arc::clone(&self.run_handle);

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create runtime: {}", e)))?;

        rt.block_on(async {
            let guard = run_handle_ref.lock().await;
            if let Some(ref handle) = *guard {
                let run_id: orchest::run::RunId = orchest::run::RunId(
                    uuid::Uuid::parse_str(&run_id_str)
                        .map_err(|e| PyRuntimeError::new_err(format!("invalid run_id: {}", e)))?,
                );
                handle
                    .respond_approval(run_id, approved)
                    .await
                    .map_err(PyRuntimeError::new_err)?;
                Ok(())
            } else {
                Err(PyRuntimeError::new_err(
                    "no active run to respond to — call run() or run_sync() first",
                ))
            }
        })
    }
}

#[pymodule]
fn orchest_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Agent>()?;
    Ok(())
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    fn initialize_python() {
        Python::initialize();
    }

    #[test]
    fn python_execution_mode_defaults_to_normal() {
        initialize_python();

        let mode = parse_execution_mode(None, None, None).expect("normal mode should parse");

        assert_eq!(mode, ToolExecutionMode::Normal);
    }

    #[test]
    fn python_execution_mode_parses_draft() {
        initialize_python();

        let mode = parse_execution_mode(Some("draft"), Some("commit_write".into()), None)
            .expect("draft mode should parse");

        assert_eq!(
            mode,
            ToolExecutionMode::Draft {
                commit_tool: "commit_write".into()
            }
        );
    }

    #[test]
    fn python_execution_mode_requires_linked_tool() {
        initialize_python();

        let err = parse_execution_mode(Some("commit"), None, None)
            .expect_err("commit mode should require draft_tool");

        assert!(err.to_string().contains("requires draft_tool"));
    }
}
