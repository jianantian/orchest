//! DeepSeek provider example: agent run with tool calling.
//!
//! Uses deepseek-v4-flash (fast, cheap) with a weather tool.
//!
//! Run with:
//!   DEEPSEEK_API_KEY=sk-... cargo run -p agent-runtime-providers --example rust_provider_runtime_deepseek

use std::sync::{Arc, LazyLock};

use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::{ModelAdapter, StreamEvent};
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use orchest_providers::{create_adapter_from_config, ProviderRuntimeConfig};
use serde_json::json;

// ── Tool ─────────────────────────────────────────────────────────────────────

struct WeatherTool;

fn weather_schema() -> &'static JsonSchema {
    static S: LazyLock<JsonSchema> = LazyLock::new(
        || json!({"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}),
    );
    &S
}

fn weather_meta() -> &'static ToolMetadata {
    static M: LazyLock<ToolMetadata> = LazyLock::new(|| ToolMetadata {
        side_effect: false,
        approval: Approval::Never,
        execution_mode: agent_runtime_core::tool::ToolExecutionMode::Normal,
        parallelism: agent_runtime_core::tool::ToolParallelism::Serial,
        cost_hint: None,
        timeout: None,
        max_output_tokens: None,
        source: ToolSource::InProcess,
    });
    &M
}

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }
    fn description(&self) -> &str {
        "Get the current weather for a city."
    }
    fn input_schema(&self) -> &JsonSchema {
        weather_schema()
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        weather_meta()
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let city = input["city"].as_str().unwrap_or("unknown");
        Ok(ToolOutput::Immediate(json!({
            "city": city, "temperature": 22, "condition": "sunny"
        })))
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model: Arc<dyn ModelAdapter> =
        Arc::from(create_adapter_from_config(ProviderRuntimeConfig {
            model: "deepseek/deepseek-v4-flash".into(),
            api_key: None,
            api_key_env: Some("DEEPSEEK_API_KEY".into()),
            api_url: None,
            max_tokens: Some(1024),
        })?);

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WeatherTool))?;

    let config = AgentConfig::builder("deepseek/deepseek-v4-flash")
        .system_prompt("You are a helpful assistant. Answer concisely.")
        .max_steps(5)
        .build()?;

    let (_handle, mut rx) = AgentRun::start(
        config,
        "What's the weather in Tokyo?".into(),
        model,
        registry,
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ModelStreamChunk {
                delta: StreamEvent::Text { delta },
            } => print!("{delta}"),
            RuntimeEvent::ToolCallStarted { tool, .. } => println!("\n[tool] calling {tool}"),
            RuntimeEvent::ToolCallCompleted { tool, output, .. } => {
                println!("[tool] {tool} -> {output}")
            }
            RuntimeEvent::RunCompleted { output } => println!("\n[done] {output}"),
            RuntimeEvent::RunFailed { error } => eprintln!("\n[failed] {error}"),
            _ => {}
        }
    }
    println!();
    Ok(())
}
