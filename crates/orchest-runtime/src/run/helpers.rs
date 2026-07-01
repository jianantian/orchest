//! Utility functions shared across run submodules.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::tool::mcp::{McpClient, McpHttpClient, McpStdioClient, McpTool, McpTransport};
use crate::tool::registry::ToolRegistry;
use crate::tool::ToolDef;

use super::config::AgentConfig;

pub(crate) async fn emit(tx: &mpsc::Sender<RuntimeEvent>, event: RuntimeEvent) {
    let _ = tx.send(event).await;
}

pub(crate) fn truncate_output(value: Value, max_tokens: u64) -> Value {
    crate::tokenizer::truncate_to_tokens(value, max_tokens as usize)
}

pub(crate) fn min_option<T: Ord + Copy>(requested: Option<T>, remaining: Option<T>) -> Option<T> {
    match (requested, remaining) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub(crate) fn min_option_f64(requested: Option<f64>, remaining: Option<f64>) -> Option<f64> {
    match (requested, remaining) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub(crate) fn append_searched_tool_defs(tool_defs: &mut Vec<ToolDef>, value: &Value) {
    let Some(results) = value.as_array() else {
        return;
    };
    for result in results {
        let Some(name) = result.get("name").and_then(Value::as_str) else {
            continue;
        };
        if tool_defs.iter().any(|tool| tool.name == name) {
            continue;
        }
        tool_defs.push(ToolDef {
            name: name.to_string(),
            description: result
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            input_schema: result
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"})),
        });
    }
}

pub(crate) async fn connect_mcp_servers(
    config: &AgentConfig,
    registry: &mut ToolRegistry,
) -> Result<(), crate::tool::mcp::McpError> {
    for server in &config.runtime.mcp_servers {
        match &server.transport {
            McpTransport::Stdio { command, args } => {
                let client = Arc::new(McpStdioClient::connect_owned(command, args).await?);
                let tools = client.list_tools().await?;
                for def in tools {
                    registry
                        .register(Arc::new(McpTool::new(
                            server.server_id.clone(),
                            def,
                            McpClient::Stdio(Arc::clone(&client)),
                        )))
                        .map_err(|e| crate::tool::mcp::McpError {
                            message: e.to_string(),
                            code: Some("registry_error".into()),
                        })?;
                }
            }
            McpTransport::StreamableHttp { url, auth } => {
                let client = Arc::new(McpHttpClient::connect(url, auth.clone()).await?);
                let tools = client.list_tools().await?;
                for def in tools {
                    registry
                        .register(Arc::new(McpTool::new(
                            server.server_id.clone(),
                            def,
                            McpClient::Http(Arc::clone(&client)),
                        )))
                        .map_err(|e| crate::tool::mcp::McpError {
                            message: e.to_string(),
                            code: Some("registry_error".into()),
                        })?;
                }
            }
        }
    }
    Ok(())
}
