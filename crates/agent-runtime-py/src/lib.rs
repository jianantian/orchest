use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use agent_runtime_core::budget::BudgetConfig;
use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::anthropic::{AnthropicAdapter, AnthropicConfig};
use agent_runtime_core::model::ModelSpec;
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::async_job::{JobHandle, JobStatus};
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};

#[pyclass]
struct Agent {
    model: String,
    system_prompt: String,
    api_url: Option<String>,
    #[allow(dead_code)]
    skills_dir: Option<String>,
    budget: Option<PyBudget>,
    tools: Vec<PyToolDef>,
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
    requires_approval: bool,
    side_effect: bool,
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
        let input_str = serde_json::to_string(&input).map_err(|e| ToolError {
            message: format!("failed to serialize input: {}", e),
            code: None,
        })?;

        let result = Python::attach(|py| -> PyResult<Py<PyAny>> {
            let json_mod = py.import("json")?;
            let input_py = json_mod.call_method1("loads", (&input_str,))?;
            let result = if let Ok(kwargs) = input_py.cast::<PyDict>() {
                callback.call(py, (), Some(kwargs))?
            } else {
                callback.call1(py, (input_py,))?
            };
            Ok(result)
        })
        .map_err(|e| ToolError {
            message: format!("Python tool error: {}", e),
            code: None,
        })?;

        if let Some(job_handle) = py_async_job_handle(&result)? {
            return Ok(ToolOutput::AsyncJob(job_handle));
        }

        let result_str = Python::attach(|py| -> PyResult<String> {
            let json_mod = py.import("json")?;
            json_mod
                .call_method1("dumps", (result.bind(py),))?
                .extract()
        })
        .map_err(|e| ToolError {
            message: format!("failed to serialize Python return value: {}", e),
            code: None,
        })?;

        let value: Value = serde_json::from_str(&result_str).map_err(|e| ToolError {
            message: format!("failed to parse Python return value: {}", e),
            code: None,
        })?;

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
                    let value = poll.call0(py).map_err(|e| ToolError {
                        message: format!("Python async job poll error: {}", e),
                        code: None,
                    })?;
                    let json_mod = py.import("json").map_err(|e| ToolError {
                        message: format!("failed to import json: {}", e),
                        code: None,
                    })?;
                    let json_str: String = json_mod
                        .call_method1("dumps", (value.bind(py),))
                        .and_then(|v| v.extract())
                        .map_err(|e| ToolError {
                            message: format!("failed to serialize poll result: {}", e),
                            code: None,
                        })?;
                    let parsed: Value = serde_json::from_str(&json_str).map_err(|e| ToolError {
                        message: format!("failed to parse poll result: {}", e),
                        code: None,
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
            poll: Arc::new(poll_fn),
            poll_interval: Duration::from_millis(poll_interval_ms),
            timeout: None,
        }))
    })
    .map_err(|e| ToolError {
        message: format!("invalid Python async job return value: {}", e),
        code: None,
    })
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

fn runtime_event_to_dict(py: Python<'_>, event: &RuntimeEvent) -> PyResult<Py<PyDict>> {
    let json_str = serde_json::to_string(event)
        .map_err(|e| PyRuntimeError::new_err(format!("failed to serialize event: {}", e)))?;
    let value: Value = serde_json::from_str(&json_str)
        .map_err(|e| PyRuntimeError::new_err(format!("failed to parse event: {}", e)))?;

    let event_obj = match value {
        Value::Object(outer) if outer.len() == 1 => {
            let (variant, fields) = outer.into_iter().next().ok_or_else(|| {
                PyRuntimeError::new_err("failed to extract serialized event variant")
            })?;
            let mut result = match fields {
                Value::Object(fields) => fields,
                other => {
                    let mut fields = serde_json::Map::new();
                    fields.insert("value".into(), other);
                    fields
                }
            };
            result.insert("type".into(), Value::String(to_snake_case(&variant)));
            Value::Object(result)
        }
        other => other,
    };
    let json_str = serde_json::to_string(&event_obj)
        .map_err(|e| PyRuntimeError::new_err(format!("failed to serialize event dict: {}", e)))?;

    let json_mod = py.import("json")?;
    let dict = json_mod.call_method1("loads", (&json_str,))?;
    let dict: Py<PyDict> = dict.extract()?;

    Ok(dict)
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[pymethods]
impl Agent {
    #[new]
    #[pyo3(signature = (model, system_prompt, skills_dir=None, budget=None, api_url=None))]
    fn new(
        model: String,
        system_prompt: String,
        skills_dir: Option<String>,
        budget: Option<Bound<'_, PyDict>>,
        api_url: Option<String>,
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

        Ok(Self {
            model,
            system_prompt,
            api_url,
            skills_dir,
            budget: py_budget,
            tools: Vec::new(),
        })
    }

    fn set_api_url(&mut self, api_url: Option<String>) {
        self.api_url = api_url;
    }

    fn tool(&mut self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let name: String = func.getattr(py, "__name__")?.extract(py)?;
        let description: String = func
            .getattr(py, "__doc__")
            .and_then(|d| d.extract(py))
            .unwrap_or_else(|_| format!("Tool: {}", name));
        let input_schema = infer_schema_from_hints(py, &func)?;

        self.tools.push(PyToolDef {
            name,
            description,
            input_schema,
            requires_approval: false,
            side_effect: false,
            callback: func.clone_ref(py),
        });

        Ok(func)
    }

    fn run<'py>(
        &self,
        py: Python<'py>,
        input: String,
    ) -> PyResult<Bound<'py, pyo3::types::PyList>> {
        let budget_config = if let Some(ref b) = self.budget {
            BudgetConfig {
                max_tokens: b.max_tokens,
                max_tool_calls: b.max_tool_calls,
                max_duration: b.max_duration_secs.map(Duration::from_secs),
                max_cost_usd: b.max_cost_usd,
            }
        } else {
            BudgetConfig {
                max_tokens: None,
                max_tool_calls: None,
                max_duration: None,
                max_cost_usd: None,
            }
        };

        let config = AgentConfig {
            system_prompt: self.system_prompt.clone(),
            model: ModelSpec {
                provider: "anthropic".into(),
                model: self.model.clone(),
                api_key_env: None,
                api_url: self.api_url.clone(),
                max_tokens: Some(4096),
            },
            budget: budget_config,
            max_steps: 20,
            allowed_skills: None,
            allowed_tools: None,
            mcp_servers: vec![],
        };

        let mut registry = ToolRegistry::new();
        for tool_def in &self.tools {
            let tool = PyTool {
                def: tool_def.clone(),
                metadata: ToolMetadata {
                    side_effect: tool_def.side_effect,
                    requires_approval: tool_def.requires_approval,
                    cost_hint: None,
                    timeout: None,
                    max_output_tokens: None,
                    source: ToolSource::InProcess,
                },
            };
            registry
                .register(Arc::new(tool))
                .map_err(|e| PyRuntimeError::new_err(format!("failed to register tool: {}", e)))?;
        }

        let model_adapter: Arc<dyn agent_runtime_core::model::ModelAdapter> = Arc::new(
            AnthropicAdapter::from_config(AnthropicConfig {
                model: self.model.clone(),
                max_tokens: 4096,
                api_key: None,
                api_url: self.api_url.clone(),
            })
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create model: {}", e)))?,
        );

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create runtime: {}", e)))?;

        let events = py.detach(|| {
            rt.block_on(async {
                let (_handle, mut event_rx) =
                    AgentRun::start(config, input, model_adapter, registry);

                let mut events = Vec::new();
                while let Some(event) = event_rx.recv().await {
                    events.push(event);
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

    fn respond_approval(&self, _run_id_str: String, _approved: bool) -> PyResult<()> {
        Err(PyRuntimeError::new_err(
            "respond_approval requires an active run handle (not yet supported in sync mode)",
        ))
    }
}

#[pymodule]
fn agent_runtime_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Agent>()?;
    Ok(())
}
