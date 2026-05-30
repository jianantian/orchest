//! AgentAsTool: wraps an AgentConfig as a standard Tool.
//!
//! The child run executes in its own tokio task (spawned by AgentRun::start_with_bus);
//! AgentAsTool::execute awaits completion and forwards every child RuntimeEvent
//! upward as SubAgentEvent, giving consumers a continuous event stream.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::budget::BudgetUsage;
use crate::events::RuntimeEvent;
use crate::model::ModelAdapter;
use crate::run::{AgentConfig, AgentRun};
use crate::tool::registry::ToolRegistry;
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

pub struct AgentAsTool {
    config: AgentConfig,
    tool_name: String,
    tool_description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    input_mapper: Arc<dyn Fn(Value) -> Result<String, ToolError> + Send + Sync>,
    output_extractor: Arc<dyn Fn(Value) -> Value + Send + Sync>,
}

impl AgentAsTool {
    #[allow(clippy::too_many_arguments)] // justified: constructor mirrors all config fields; a builder pattern is planned for v0.8
    pub fn new(
        config: AgentConfig,
        tool_name: String,
        tool_description: String,
        input_schema: JsonSchema,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        input_mapper: Arc<dyn Fn(Value) -> Result<String, ToolError> + Send + Sync>,
        output_extractor: Arc<dyn Fn(Value) -> Value + Send + Sync>,
    ) -> Self {
        Self {
            config,
            tool_name,
            tool_description,
            input_schema,
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            },
            model,
            registry,
            input_mapper,
            output_extractor,
        }
    }
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

        let (handle, mut child_rx) = AgentRun::start_with_bus(
            child_config,
            child_input.clone(),
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
        })
    }
}
