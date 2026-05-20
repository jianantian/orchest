use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use napi_derive::napi;
use serde_json::Value;

use agent_runtime_core::budget::BudgetConfig;
use agent_runtime_core::model::anthropic::{AnthropicAdapter, AnthropicConfig};
use agent_runtime_core::model::ModelSpec;
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};

#[napi(object)]
pub struct AgentOptions {
    pub model: String,
    pub system_prompt: String,
    pub skills_dir: Option<String>,
    pub api_url: Option<String>,
    pub budget: Option<BudgetOptions>,
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
        Ok(ToolOutput::Immediate(serde_json::json!({
            "error": "JS callback tools not yet wired in v0.1 — use Rust tools or skill bundled scripts"
        })))
    }
}

#[napi]
pub struct Agent {
    model: String,
    system_prompt: String,
    api_url: Option<String>,
    #[allow(dead_code)]
    skills_dir: Option<String>,
    budget: Option<BudgetOptions>,
    tools: Vec<Arc<dyn Tool>>,
}

#[napi]
impl Agent {
    #[napi(constructor)]
    pub fn new(options: AgentOptions) -> Self {
        Self {
            model: options.model,
            system_prompt: options.system_prompt,
            api_url: options.api_url,
            skills_dir: options.skills_dir,
            budget: options.budget,
            tools: Vec::new(),
        }
    }

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

    #[napi]
    pub fn run_sync(&self, input: String) -> napi::Result<Vec<serde_json::Value>> {
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
        for tool in &self.tools {
            registry
                .register(Arc::clone(tool))
                .map_err(|e| napi::Error::from_reason(format!("{}", e)))?;
        }

        let model: Arc<dyn agent_runtime_core::model::ModelAdapter> = Arc::new(
            AnthropicAdapter::from_config(AnthropicConfig {
                model: self.model.clone(),
                max_tokens: 4096,
                api_key: None,
                api_url: self.api_url.clone(),
            })
            .map_err(|e| napi::Error::from_reason(format!("{}", e)))?,
        );

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| napi::Error::from_reason(format!("failed to create runtime: {}", e)))?;

        let events = rt.block_on(async {
            let (_handle, mut event_rx) = AgentRun::start(config, input, model, registry);

            let mut events = Vec::new();
            while let Some(event) = event_rx.recv().await {
                events.push(event);
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

    #[napi]
    pub fn respond_approval(&self, _run_id: String, _approved: bool) -> napi::Result<()> {
        Err(napi::Error::from_reason(
            "respondApproval requires an active run handle (not yet supported)",
        ))
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
}
