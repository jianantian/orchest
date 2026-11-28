//! Native tool registrations exposed by both SDKs.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::run::ConfigError;
use crate::tool::builtin::WriteFileTool;
use crate::tool::{Approval, Tool, ToolError};

use super::run::BindingAgent;

pub fn agent_tool(
    name: &str,
    description: &str,
    agent: BindingAgent,
    input_key: Option<String>,
) -> Result<Arc<dyn Tool>, ConfigError> {
    let BindingAgent {
        config,
        model,
        registry,
    } = agent;
    let input_key = input_key.unwrap_or_else(|| "question".into());
    let schema = json!({
        "type": "object",
        "properties": {
            input_key.clone(): {
                "type": "string",
                "description": "Task or question to delegate to the child agent"
            }
        },
        "required": [input_key.clone()]
    });
    config
        .as_tool(name, description)
        .model(model)
        .registry(registry)
        .input_schema(schema)
        .input_mapper(move |value| {
            value
                .get(&input_key)
                .and_then(Value::as_str)
                .map(String::from)
                .ok_or_else(|| {
                    ToolError::fatal(format!("missing required parameter '{input_key}'"))
                        .with_code("MISSING_PARAM")
                })
        })
        .output_extractor(|details| details.get("output").cloned().unwrap_or(details))
        .build()
}

pub fn write_file_tool(approval: Option<&str>) -> Arc<dyn Tool> {
    Arc::new(WriteFileTool::new_with_approval(
        super::parse_binding_approval(approval, Approval::Always),
    ))
}
