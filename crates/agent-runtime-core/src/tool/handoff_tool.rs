//! HandoffTool: exposes a Handoff as a callable Tool.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::handoff::{Handoff, HandoffResult, HandoffTarget};
use crate::tool::{CostHint, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

pub struct HandoffTool {
    pub handoff: Handoff,
    metadata: ToolMetadata,
}

impl HandoffTool {
    pub fn new(handoff: Handoff) -> Self {
        let metadata = ToolMetadata {
            side_effect: false,
            requires_approval: false,
            cost_hint: Some(CostHint::Free),
            timeout: Some(Duration::from_secs(30)),
            max_output_tokens: None,
            source: ToolSource::Builtin,
        };
        Self { handoff, metadata }
    }
}

#[async_trait]
impl Tool for HandoffTool {
    fn name(&self) -> &str {
        &self.handoff.tool_name
    }

    fn description(&self) -> &str {
        &self.handoff.tool_description
    }

    fn input_schema(&self) -> &Value {
        &self.handoff.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let target_agent = match &self.handoff.target {
            HandoffTarget::Static(config) => *config.clone(),
            HandoffTarget::Dynamic(resolver) => {
                resolver
                    .resolve(input.clone())
                    .await
                    .map_err(|e| ToolError {
                        message: e.to_string(),
                        code: None,
                    })?
            }
        };

        let transfer_message = format!("Transferring session to '{}'.", &self.handoff.tool_name);

        Ok(ToolOutput::Handoff(Box::new(HandoffResult {
            target_agent,
            transfer_message,
            input_filter: self.handoff.input_filter.clone(),
            nest_history: self.handoff.nest_history,
        })))
    }
}

impl From<Handoff> for Arc<dyn Tool> {
    fn from(handoff: Handoff) -> Arc<dyn Tool> {
        Arc::new(HandoffTool::new(handoff))
    }
}
