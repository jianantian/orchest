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
use crate::run::{AgentConfig, AgentRun};
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
            return Ok(ToolOutput::Immediate(json!({
                "child_run_id": child_run_id,
                "error": "max_run_depth_exceeded",
                "budget_used": BudgetUsage::default(),
            })));
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
            child_input.clone(),
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

        let details = if let Some(error) = failed {
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(RuntimeEvent::SubAgentFailed {
                        child_run_id,
                        error: error.clone(),
                    })
                    .await;
            }
            json!({
                "child_run_id": child_run_id,
                "error": error,
                "budget_used": child_usage,
            })
        } else {
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(RuntimeEvent::SubAgentCompleted {
                        child_run_id,
                        output: output.clone(),
                        budget_used: child_usage.clone(),
                    })
                    .await;
            }
            json!({
                "child_run_id": child_run_id,
                "output": output,
                "budget_used": child_usage,
            })
        };

        let model_output = (self.output_extractor)(details.clone());
        Ok(ToolOutput::Structured {
            model_output,
            details,
            external_usage: Some(child_usage),
        })
    }
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

    pub fn build(self) -> Arc<dyn Tool> {
        let model = self
            .model
            .expect("SubAgentBuilder requires .model() before .build()");
        let registry = self
            .registry
            .expect("SubAgentBuilder requires .registry() before .build()");
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

        Arc::new(AgentAsTool {
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
        })
    }
}
