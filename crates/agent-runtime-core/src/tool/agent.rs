//! AgentTool: tool implementation that delegates to a sub-agent.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::model::ModelAdapter;
use crate::run::AgentConfig;
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    AgentDelegate, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};

pub struct AgentTool {
    name: String,
    description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    config: AgentConfig,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    input_mapper: Arc<dyn Fn(Value) -> Result<String, ToolError> + Send + Sync>,
    output_mapper: Arc<dyn Fn(Value) -> Value + Send + Sync>,
}

impl AgentTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        description: String,
        input_schema: JsonSchema,
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        input_mapper: Arc<dyn Fn(Value) -> Result<String, ToolError> + Send + Sync>,
        output_mapper: Arc<dyn Fn(Value) -> Value + Send + Sync>,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            },
            config,
            model,
            registry,
            input_mapper,
            output_mapper,
        }
    }
}

#[async_trait]
impl Tool for AgentTool {
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
        let mapped_input = (self.input_mapper)(input)?;
        Ok(ToolOutput::AgentDelegate(Box::new(AgentDelegate {
            input: mapped_input,
            config: self.config.clone(),
            model: Arc::clone(&self.model),
            registry: self.registry.clone(),
            output_mapper: Arc::clone(&self.output_mapper),
        })))
    }
}
