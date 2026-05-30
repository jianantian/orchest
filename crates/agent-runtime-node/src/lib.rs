use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;

use agent_runtime_core::budget::BudgetConfig;
use agent_runtime_core::model::{
    CachePolicy, CompatibilityPolicy, ModelSpec, ProviderRuntimeConfig,
    RequestOptions as RustRequestOptions, ThinkingLevel,
};
use agent_runtime_core::run::{
    AgentConfig, AgentRun, ModelConfig, RunHandle, RuntimeConfig, SkillsConfig,
};
use agent_runtime_core::tool::async_job::JobHandle;
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use agent_runtime_providers::{create_adapter_from_config, normalize_provider_model};

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
    pub requires_approval: Option<bool>,
    pub side_effect: Option<bool>,
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

// Safety: ThreadsafeFunction is designed to be Send + Sync
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

        let result = rx.await.map_err(|_| ToolError {
            message: "JS handler did not return a value".into(),
            code: Some("JS_HANDLER_ERROR".into()),
        })?;

        let value = result.map_err(|e| ToolError {
            message: e,
            code: Some("JS_HANDLER_ERROR".into()),
        })?;

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
        Err(ToolError {
            message: format!(
                "tool '{}' has no handler — register with registerToolWithHandler",
                self.name
            ),
            code: Some("NO_HANDLER".into()),
        })
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
            tools: Vec::new(),
            run_handle: Arc::new(TokioMutex::new(None)),
        })
    }

    /// Register a tool with schema only (no handler — tool calls will error).
    /// For tools with handlers, use `registerToolWithHandler`.
    #[napi]
    pub fn register_tool(&mut self, options: ToolRegistration) -> napi::Result<()> {
        let tool = StaticTool {
            name: options.name.clone(),
            description: options.description.clone(),
            input_schema: options.input_schema,
            metadata: ToolMetadata {
                side_effect: options.side_effect.unwrap_or(false),
                requires_approval: options.requires_approval.unwrap_or(false),
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            },
        };

        self.tools.push(Arc::new(tool));
        Ok(())
    }

    /// Register a tool with an executable JavaScript handler function.
    /// The handler receives the parsed input object and should return a
    /// JSON-serializable value. Errors thrown become ToolCallFailed events.
    #[napi(
        ts_args_type = "name: string, description: string, inputSchema: Record<string, unknown>, handler: (input: any) => any, options?: { requiresApproval?: boolean, sideEffect?: boolean }"
    )]
    #[allow(clippy::too_many_arguments)] // justified: NAPI binding mirrors JS API surface; cannot reduce without breaking TypeScript contract
    pub fn register_tool_with_handler(
        &mut self,
        name: String,
        description: String,
        input_schema: serde_json::Value,
        handler: napi::JsFunction,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let requires_approval = options
            .as_ref()
            .and_then(|o| o.get("requiresApproval"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let side_effect = options
            .as_ref()
            .and_then(|o| o.get("sideEffect"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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
                requires_approval,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            },
            handler: tsfn,
        };

        self.tools.push(Arc::new(tool));
        Ok(())
    }

    #[napi]
    pub fn run_sync(&self, input: String) -> napi::Result<Vec<serde_json::Value>> {
        let config = self.build_config()?;

        let mut registry = ToolRegistry::new();
        for tool in &self.tools {
            registry
                .register(Arc::clone(tool))
                .map_err(|e| napi::Error::from_reason(format!("{}", e)))?;
        }

        let model: Arc<dyn agent_runtime_core::model::ModelAdapter> = Arc::from(
            create_adapter_from_config(self.provider_config())
                .map_err(|e| napi::Error::from_reason(format!("failed to create model: {e}")))?,
        );

        let run_handle_ref = Arc::clone(&self.run_handle);

        let rt = shared_runtime();

        let events = rt.block_on(async {
            let (handle, mut event_rx) = AgentRun::start(config, input, model, registry);

            // Store handle for respond_approval
            {
                let mut guard = run_handle_ref.lock().await;
                *guard = Some(handle);
            }

            let mut events = Vec::new();
            while let Some(event) = event_rx.recv().await {
                events.push(event);
            }

            // Wait and clear handle
            {
                let mut guard = run_handle_ref.lock().await;
                if let Some(h) = guard.take() {
                    h.wait().await;
                }
            }

            events
        });

        let mut result = Vec::new();
        for event in &events {
            let value = serde_json::to_value(event)
                .map_err(|e| napi::Error::from_reason(format!("serialize error: {}", e)))?;
            result.push(runtime_event_to_value(value));
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

        let model: Arc<dyn agent_runtime_core::model::ModelAdapter> = Arc::from(
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
                let value = serde_json::to_value(&event)
                    .map_err(|e| napi::Error::from_reason(format!("serialize error: {}", e)))?;
                let value = runtime_event_to_value(value);
                tsfn.call(value, ThreadsafeFunctionCallMode::NonBlocking);
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
                let run_id = agent_runtime_core::run::RunId(
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
        let budget_config = if let Some(ref b) = self.budget {
            BudgetConfig {
                max_tokens: b.max_tokens.map(|v| v as u64),
                max_tool_calls: b.max_tool_calls.map(|v| v as u32),
                max_duration: b.max_duration_secs.map(|v| Duration::from_secs(v as u64)),
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
            runtime: RuntimeConfig::default(),
            hooks: vec![],
            retry_policy: None,
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

fn runtime_event_to_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(outer) if outer.len() == 1 => {
            let Some((variant, fields)) = outer.into_iter().next() else {
                return serde_json::Value::Object(serde_json::Map::new());
            };
            let mut result = match fields {
                serde_json::Value::Object(fields) => fields,
                other => {
                    let mut fields = serde_json::Map::new();
                    fields.insert("value".into(), other);
                    fields
                }
            };
            result.insert(
                "type".into(),
                serde_json::Value::String(to_snake_case(&variant)),
            );
            result
                .entry("runDepth")
                .or_insert(serde_json::Value::from(0));
            result
                .entry("childRunId")
                .or_insert(serde_json::Value::Null);
            serde_json::Value::Object(result)
        }
        other => other,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_event_type_uses_snake_case_wire_format() {
        let event = serde_json::json!({
            "ModelStreamChunk": {
                "delta": { "Text": { "delta": "hello" } }
            }
        });

        let converted = runtime_event_to_value(event);

        assert_eq!(converted["type"], "model_stream_chunk");
        assert!(converted.get("delta").is_some());
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
        })
        .expect("agent should construct");
        let config = agent.build_config().expect("config should build");
        assert_eq!(config.model.spec.provider, "anthropic");
        assert_eq!(config.model.spec.model, "claude-sonnet-4");
    }
}
