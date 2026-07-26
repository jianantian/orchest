//! AgentAsTool: wraps an AgentConfig as a standard Tool.
//!
//! The child run executes in its own tokio task (spawned by AgentRun::start_with_bus);
//! AgentAsTool::execute awaits completion and forwards every child RuntimeEvent
//! upward as SubAgentEvent, giving consumers a continuous event stream.

use std::num::NonZeroUsize;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::events::RuntimeEvent;
use crate::model::ModelAdapter;
use crate::run::{AgentConfig, AgentRun, ConfigError, RunInput};
use crate::tool::registry::ToolRegistry;
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

type InputMapperFn = dyn Fn(Value) -> Result<String, ToolError> + Send + Sync;
type OutputExtractorFn = dyn Fn(Value) -> Value + Send + Sync;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContextMode {
    #[default]
    Fresh,
    Fork {
        depth: NonZeroUsize,
    },
}

/// Returns a `BudgetConfig` whose each limit is the tightest of `configured` and `remaining`.
/// A `None` on either side means "no limit from that side", so the other side wins.
fn cap_budget(configured: BudgetConfig, remaining: &BudgetConfig) -> BudgetConfig {
    fn min_opt<T: Ord>(a: Option<T>, b: Option<T>) -> Option<T> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    fn min_opt_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    fn min_opt_dur(
        a: Option<std::time::Duration>,
        b: Option<std::time::Duration>,
    ) -> Option<std::time::Duration> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    BudgetConfig {
        max_tokens: min_opt(configured.max_tokens, remaining.max_tokens),
        max_tool_calls: min_opt(configured.max_tool_calls, remaining.max_tool_calls),
        max_duration: min_opt_dur(configured.max_duration, remaining.max_duration),
        max_cost_usd: min_opt_f64(configured.max_cost_usd, remaining.max_cost_usd),
    }
}

/// An [`AgentConfig`] wrapped as a standard [`Tool`]. The child run executes in
/// its own tokio task; every child [`RuntimeEvent`] is forwarded upward as
/// `SubAgentEvent`, and lifecycle events (`SubAgentStarted` /
/// `SubAgentCompleted` / `SubAgentFailed`) bracket the call.
///
/// # Failure semantics (v0.15)
///
/// A failed child run (`RuntimeEvent::RunFailed`) makes [`Tool::execute`]
/// return `Err(ToolError)` — never an `Ok` payload with an embedded `"error"`
/// key — so consumers dispatch on the standard v0.9.4 `ErrorKind`/`RetryHint`
/// contract instead of scraping `details["error"]`. `SubAgentFailed` still
/// fires with the same `child_run_id` and error, and the `ToolError` message
/// carries the `child_run_id` plus the budget the child consumed before
/// failing. `ToolError` has no details/diagnostic payload field, so the budget
/// rides in the message text. The failure path never constructs
/// `ToolOutput::Structured` and never invokes `output_extractor`.
///
/// `RunFailed` carries only an opaque error string, so the kind/code
/// adjudication keys off the runtime-generated failure messages
/// (`run/actor.rs`):
///
/// | child failure                    | kind  | retry  | code                     |
/// |----------------------------------|-------|--------|--------------------------|
/// | `budget_exceeded: …`             | Fatal | Unsafe | `BUDGET_EXCEEDED`        |
/// | `max_steps_reached`              | Fatal | Unsafe | `MAX_STEPS_REACHED`      |
/// | depth guard (`run_depth >= 3`)   | Fatal | Unsafe | `MAX_RUN_DEPTH_EXCEEDED` |
/// | anything else                    | Fatal | Unsafe | `SUB_AGENT_RUN_FAILED`   |
///
/// Every class is `Fatal`/`Unsafe` — the v0.9.4 retry dispatch never
/// auto-retries the tool — because each identifiable cause is deterministic
/// for the same input and config: a retry hits the same budget/step/depth
/// ceiling (mirroring the run loop's own
/// `ToolError::fatal(..).with_code("BUDGET_EXCEEDED")` budget convention), and
/// model-side errors surface as `RunFailed` only after the child's own retry
/// policy is exhausted, so a parent-side auto-retry would blindly re-run the
/// whole child. The parent model still receives the structured error as the
/// tool result and may deliberately re-invoke the tool with adjusted input.
pub struct AgentAsTool {
    config: AgentConfig,
    tool_name: String,
    tool_description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    input_mapper: Arc<InputMapperFn>,
    output_extractor: Arc<OutputExtractorFn>,
    context_mode: ContextMode,
}

#[async_trait]
impl Tool for AgentAsTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
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

    fn needs_parent_context(&self) -> bool {
        matches!(self.context_mode, ContextMode::Fork { .. })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let child_input = (self.input_mapper)(input)?;

        let parent_run_id = ctx.run_id;

        if ctx.run_depth >= 3 {
            let child_run_id = crate::run::RunId::new();
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(RuntimeEvent::SubAgentFailed {
                        child_run_id,
                        error: "max_run_depth_exceeded".into(),
                    })
                    .await;
            }
            return Err(ToolError::fatal(format!(
                "sub-agent run {child_run_id} failed: max_run_depth_exceeded"
            ))
            .with_code("MAX_RUN_DEPTH_EXCEEDED"));
        }

        let mut child_config = self.config.clone();
        child_config.runtime.run_depth = ctx.run_depth + 1;
        child_config.budget = cap_budget(child_config.budget, &ctx.remaining_budget);

        let initial_messages = match self.context_mode {
            ContextMode::Fresh => Vec::new(),
            ContextMode::Fork { depth } => {
                if ctx.parent_messages.is_empty() {
                    return Err(ToolError::invalid_input(
                        "ContextMode::Fork requires parent message history",
                    )
                    .with_code("EMPTY_PARENT_CONTEXT"));
                }
                ctx.parent_messages
                    .iter()
                    .rev()
                    .take(depth.get())
                    .rev()
                    .cloned()
                    .collect()
            }
        };

        let (handle, mut child_rx) = AgentRun::start_with_bus(
            child_config,
            RunInput::text(child_input.clone()).into_blocks(),
            initial_messages,
            Arc::clone(&self.model),
            self.registry.clone(),
            ctx.approval_bus.clone(),
        );
        let child_run_id = handle.run_id;

        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(RuntimeEvent::SubAgentStarted {
                    parent_run_id,
                    child_run_id,
                    config_summary: json!({
                        "run_depth": ctx.run_depth + 1,
                        "input": child_input,
                    }),
                })
                .await;
        }

        let mut child_usage = BudgetUsage::default();
        let mut output = Value::Null;
        let mut failed: Option<String> = None;

        while let Some(event) = child_rx.recv().await {
            match &event {
                RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                    let tokens_used = tokens.input_tokens + tokens.output_tokens;
                    let cost_usd = tokens.cost_usd.unwrap_or(0.0);
                    child_usage.tokens_used += tokens_used;
                    child_usage.cost_usd += cost_usd;
                }
                RuntimeEvent::ToolCallCompleted { .. } => {
                    child_usage.tool_calls_used += 1;
                }
                RuntimeEvent::RunCompleted {
                    output: child_output,
                    ..
                } => {
                    output = child_output.clone();
                }
                RuntimeEvent::RunFailed { error } => {
                    failed = Some(error.clone());
                }
                _ => {}
            }
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(RuntimeEvent::SubAgentEvent {
                        parent_run_id,
                        child_run_id,
                        event: Box::new(event),
                    })
                    .await;
            }
        }
        handle.wait().await;

        if let Some(error) = failed {
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(RuntimeEvent::SubAgentFailed {
                        child_run_id,
                        error: error.clone(),
                    })
                    .await;
            }
            return Err(child_failure_error(child_run_id, &error, &child_usage));
        }

        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(RuntimeEvent::SubAgentCompleted {
                    child_run_id,
                    output: output.clone(),
                    budget_used: child_usage.clone(),
                })
                .await;
        }
        let details = json!({
            "child_run_id": child_run_id,
            "output": output,
            "budget_used": child_usage,
        });

        let model_output = (self.output_extractor)(details.clone());
        Ok(ToolOutput::Structured {
            model_output,
            details,
            external_usage: Some(child_usage),
        })
    }
}

/// Builds the `ToolError` for a failed child run. `RunFailed` carries only an
/// opaque error string, so the code adjudication keys off the
/// runtime-generated failure messages (`run/actor.rs`); the kind is always
/// `Fatal` with `RetryHint::Unsafe`. See the [`AgentAsTool`] docs for the full
/// mapping rule and rationale.
fn child_failure_error(
    child_run_id: crate::run::RunId,
    error: &str,
    budget_used: &BudgetUsage,
) -> ToolError {
    let code = if error.starts_with("budget_exceeded") {
        "BUDGET_EXCEEDED"
    } else if error == "max_steps_reached" {
        "MAX_STEPS_REACHED"
    } else {
        "SUB_AGENT_RUN_FAILED"
    };
    ToolError::fatal(format!(
        "sub-agent run {child_run_id} failed: {error} (budget_used: {} tokens, {} tool calls, ${:.4})",
        budget_used.tokens_used, budget_used.tool_calls_used, budget_used.cost_usd
    ))
    .with_code(code)
}

// ── SubAgentBuilder ──────────────────────────────────────────────────────────

pub struct SubAgentBuilder {
    config: AgentConfig,
    tool_name: String,
    tool_description: String,
    model: Option<Arc<dyn ModelAdapter>>,
    registry: Option<ToolRegistry>,
    input_schema: Option<JsonSchema>,
    input_mapper: Option<Arc<InputMapperFn>>,
    output_extractor: Option<Arc<OutputExtractorFn>>,
    context_mode: ContextMode,
}

impl SubAgentBuilder {
    pub(crate) fn new(config: AgentConfig, name: String, description: String) -> Self {
        Self {
            config,
            tool_name: name,
            tool_description: description,
            model: None,
            registry: None,
            input_schema: None,
            input_mapper: None,
            output_extractor: None,
            context_mode: ContextMode::Fresh,
        }
    }

    pub fn model(mut self, model: Arc<dyn ModelAdapter>) -> Self {
        self.model = Some(model);
        self
    }

    pub fn registry(mut self, registry: ToolRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn input_schema(mut self, schema: Value) -> Self {
        self.input_schema = Some(schema);
        self
    }

    pub fn input_mapper(
        mut self,
        f: impl Fn(Value) -> Result<String, ToolError> + Send + Sync + 'static,
    ) -> Self {
        self.input_mapper = Some(Arc::new(f));
        self
    }

    pub fn output_extractor(mut self, f: impl Fn(Value) -> Value + Send + Sync + 'static) -> Self {
        self.output_extractor = Some(Arc::new(f));
        self
    }

    pub fn context_mode(mut self, mode: ContextMode) -> Self {
        self.context_mode = mode;
        self
    }

    pub fn build(self) -> Result<Arc<dyn Tool>, ConfigError> {
        let model = self.model.ok_or(ConfigError::SubAgentMissingModel)?;
        let registry = self.registry.ok_or(ConfigError::SubAgentMissingRegistry)?;
        let input_schema = self.input_schema.unwrap_or_else(
            || json!({"type": "object", "properties": {"input": {"type": "string"}}}),
        );
        let input_mapper = self.input_mapper.unwrap_or_else(|| {
            Arc::new(|input: Value| {
                input
                    .get("input")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| ToolError::fatal("missing 'input' field"))
            })
        });
        let output_extractor = self
            .output_extractor
            .unwrap_or_else(|| Arc::new(|details: Value| details));

        Ok(Arc::new(AgentAsTool {
            config: self.config,
            tool_name: self.tool_name,
            tool_description: self.tool_description,
            input_schema,
            metadata: ToolMetadata {
                side_effect: false,
                approval: crate::tool::Approval::Never,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
            model,
            registry,
            input_mapper,
            output_extractor,
            context_mode: self.context_mode,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ContentBlock, ModelCapabilities, ModelError, ModelResponse, RequestOptions, StopReason,
        TokenUsage,
    };
    use crate::tool::{ErrorKind, RetryHint};
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::mpsc as tokio_mpsc;

    struct NeverCalledModel;

    #[async_trait]
    impl ModelAdapter for NeverCalledModel {
        fn provider_name(&self) -> &str {
            "never-called"
        }
        fn model_name(&self) -> &str {
            "never-called"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            unimplemented!("build() never invokes the model")
        }
    }

    fn test_agent_config() -> AgentConfig {
        AgentConfig::builder("mock/model")
            .system_prompt("test")
            .max_steps(1)
            .build()
            .unwrap()
    }

    #[test]
    fn build_fails_with_missing_model_when_model_not_set() {
        let result = test_agent_config()
            .as_tool("t", "d")
            .registry(ToolRegistry::new())
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("build() should fail without .model()"),
        };
        assert!(matches!(err, ConfigError::SubAgentMissingModel));
    }

    #[test]
    fn build_fails_with_missing_registry_when_registry_not_set() {
        let result = test_agent_config()
            .as_tool("t", "d")
            .model(Arc::new(NeverCalledModel))
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("build() should fail without .registry()"),
        };
        assert!(matches!(err, ConfigError::SubAgentMissingRegistry));
    }

    #[test]
    fn build_succeeds_when_model_and_registry_both_set() {
        let tool = test_agent_config()
            .as_tool("t", "d")
            .model(Arc::new(NeverCalledModel))
            .registry(ToolRegistry::new())
            .build()
            .unwrap();
        assert_eq!(tool.name(), "t");
    }

    // ── execute: failure → Err(ToolError), success → Structured ─────────────

    /// Child model that fails the run immediately: with no retry policy
    /// configured the child emits `RunFailed { error: "provider exploded" }`.
    struct FailingModel;

    #[async_trait]
    impl ModelAdapter for FailingModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "failing"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError::internal("provider exploded", "TEST_BOOM"))
        }
    }

    /// Child model that always requests an unregistered tool, so the run keeps
    /// looping until a limit (max_steps or budget) fails it.
    struct ToolUseLoopModel;

    #[async_trait]
    impl ModelAdapter for ToolUseLoopModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "loop"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "loop-1".into(),
                    name: "missing_tool".into(),
                    input: json!({}),
                }],
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 3,
                    ..Default::default()
                },
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }

    /// Child model that answers immediately with text.
    struct SuccessModel;

    #[async_trait]
    impl ModelAdapter for SuccessModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "success"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("child answer".into())],
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 3,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }

    fn build_child_tool(
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        output_extractor: impl Fn(Value) -> Value + Send + Sync + 'static,
    ) -> Arc<dyn Tool> {
        config
            .as_tool("child", "child under test")
            .model(model)
            .registry(ToolRegistry::new())
            .output_extractor(output_extractor)
            .build()
            .unwrap()
    }

    fn execute_ctx(run_depth: u32) -> (ToolContext, tokio_mpsc::Receiver<RuntimeEvent>) {
        let (tx, rx) = tokio_mpsc::channel(64);
        (
            ToolContext {
                run_id: crate::run::RunId::new(),
                run_depth,
                tool_call_id: "test-call".into(),
                event_tx: Some(tx),
                webhook_base_url: None,
                approval_bus: crate::run::ApprovalBus::default(),
                remaining_budget: crate::budget::BudgetConfig::default(),
                parent_messages: vec![],
            },
            rx,
        )
    }

    fn drain_events(mut rx: tokio_mpsc::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn child_run_failed_returns_err_with_diagnostics() {
        let extractor_calls = Arc::new(AtomicU32::new(0));
        let calls = Arc::clone(&extractor_calls);
        let tool = build_child_tool(
            test_agent_config(),
            Arc::new(FailingModel),
            move |details| {
                calls.fetch_add(1, Ordering::SeqCst);
                details
            },
        );
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "boom"}), &ctx)
            .await
            .expect_err("child RunFailed must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("SUB_AGENT_RUN_FAILED"));
        assert!(
            err.message.contains("provider exploded"),
            "message carries the child error: {}",
            err.message
        );
        assert!(
            err.message.contains("budget_used"),
            "message carries budget diagnostics: {}",
            err.message
        );

        let events = drain_events(rx);
        let (child_run_id, error) = events
            .iter()
            .find_map(|e| match e {
                RuntimeEvent::SubAgentFailed {
                    child_run_id,
                    error,
                } => Some((*child_run_id, error.clone())),
                _ => None,
            })
            .expect("SubAgentFailed must still fire");
        assert_eq!(error, "provider exploded");
        assert!(
            err.message.contains(&child_run_id.to_string()),
            "message carries child_run_id: {}",
            err.message
        );
        assert_eq!(
            extractor_calls.load(Ordering::SeqCst),
            0,
            "output_extractor must not be invoked on failure"
        );
    }

    #[tokio::test]
    async fn child_budget_exceeded_maps_to_budget_exceeded_code() {
        let mut config = test_agent_config();
        config.runtime.max_steps = 5;
        config.budget.max_tokens = Some(1);
        let tool = build_child_tool(config, Arc::new(ToolUseLoopModel), |details| details);
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "loop"}), &ctx)
            .await
            .expect_err("child budget exhaustion must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("BUDGET_EXCEEDED"));
        assert!(
            err.message.contains("budget_exceeded"),
            "message carries the child error: {}",
            err.message
        );
        // The child consumed 5 tokens before the guard fired; the diagnostic
        // must report real usage, not a default.
        assert!(
            err.message.contains("5 tokens"),
            "message carries consumed budget: {}",
            err.message
        );

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::SubAgentFailed { error, .. } if error.starts_with("budget_exceeded"))
        ));
    }

    #[tokio::test]
    async fn child_max_steps_maps_to_max_steps_reached_code() {
        // test_agent_config caps at max_steps(1); a model that always requests
        // another tool call hits the step ceiling instead of completing.
        let tool = build_child_tool(test_agent_config(), Arc::new(ToolUseLoopModel), |details| {
            details
        });
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "loop"}), &ctx)
            .await
            .expect_err("child max-steps exhaustion must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("MAX_STEPS_REACHED"));
        assert!(
            err.message.contains("max_steps_reached"),
            "message carries the child error: {}",
            err.message
        );

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::SubAgentFailed { error, .. } if error == "max_steps_reached")
        ));
    }

    #[tokio::test]
    async fn run_depth_guard_returns_err_without_starting_child() {
        let extractor_calls = Arc::new(AtomicU32::new(0));
        let calls = Arc::clone(&extractor_calls);
        let tool = build_child_tool(
            test_agent_config(),
            Arc::new(NeverCalledModel),
            move |details| {
                calls.fetch_add(1, Ordering::SeqCst);
                details
            },
        );
        let (ctx, rx) = execute_ctx(3);

        let err = tool
            .execute(json!({"input": "too deep"}), &ctx)
            .await
            .expect_err("depth guard must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("MAX_RUN_DEPTH_EXCEEDED"));
        assert!(
            err.message.contains("max_run_depth_exceeded"),
            "message: {}",
            err.message
        );

        let events = drain_events(rx);
        let child_run_id = events
            .iter()
            .find_map(|e| match e {
                RuntimeEvent::SubAgentFailed {
                    child_run_id,
                    error,
                } if error == "max_run_depth_exceeded" => Some(*child_run_id),
                _ => None,
            })
            .expect("SubAgentFailed must still fire for the depth guard");
        assert!(err.message.contains(&child_run_id.to_string()));
        assert_eq!(extractor_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn child_success_returns_structured_with_details_and_external_usage() {
        let extractor_calls = Arc::new(AtomicU32::new(0));
        let calls = Arc::clone(&extractor_calls);
        let tool = build_child_tool(
            test_agent_config(),
            Arc::new(SuccessModel),
            move |details| {
                calls.fetch_add(1, Ordering::SeqCst);
                json!({"extracted": details.get("output").cloned().unwrap_or(Value::Null)})
            },
        );
        let (ctx, rx) = execute_ctx(0);

        let output = tool
            .execute(json!({"input": "hi"}), &ctx)
            .await
            .expect("successful child run returns Ok");

        let (model_output, details, external_usage) = match output {
            ToolOutput::Structured {
                model_output,
                details,
                external_usage,
            } => (model_output, details, external_usage),
            other => panic!("expected Structured, got {other:?}"),
        };
        assert_eq!(model_output, json!({"extracted": "child answer"}));
        assert_eq!(details["output"], json!("child answer"));
        assert!(details.get("child_run_id").is_some());
        assert_eq!(details["budget_used"]["tokens_used"], json!(5));
        let usage = external_usage.expect("external_usage carries child budget");
        assert_eq!(usage.tokens_used, 5);
        assert_eq!(extractor_calls.load(Ordering::SeqCst), 1);

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::SubAgentCompleted { output, .. } if output == &json!("child answer"))
        ));
        assert!(!events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SubAgentFailed { .. })));
    }
}
