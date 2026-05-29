//! In-process tool wrapper for closure-based tool implementations.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput};

pub type ToolCallback = Arc<
    dyn Fn(
            Value,
            ToolContext,
        ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send>>
        + Send
        + Sync,
>;

pub struct InProcessTool {
    name: String,
    description: String,
    input_schema: JsonSchema,
    output_schema: Option<JsonSchema>,
    metadata: ToolMetadata,
    callback: ToolCallback,
}

impl InProcessTool {
    #[allow(clippy::too_many_arguments)] // justified: tool construction requires all field values; builder pattern would be over-engineering
    pub fn new(
        name: String,
        description: String,
        input_schema: JsonSchema,
        output_schema: Option<JsonSchema>,
        metadata: ToolMetadata,
        callback: ToolCallback,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            output_schema,
            metadata,
            callback,
        }
    }
}

#[async_trait]
impl Tool for InProcessTool {
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
        self.output_schema.as_ref()
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let ctx_owned = ToolContext {
            run_id: ctx.run_id,
            run_depth: ctx.run_depth,
            tool_call_id: ctx.tool_call_id.clone(),
            on_update: ctx.on_update.clone(),
            event_tx: ctx.event_tx.clone(),
            webhook_base_url: ctx.webhook_base_url.clone(),
        };
        (self.callback)(input, ctx_owned).await
    }
}
