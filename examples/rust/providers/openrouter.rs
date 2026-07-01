//! OpenRouter provider example: Claude via OpenRouter with tool calling.
//!
//! Run with:
//!   OPENROUTER_API_KEY=sk-... cargo run -p agent-runtime-providers --example rust_provider_runtime_openrouter

use std::sync::{Arc, LazyLock};

use orchest_runtime::events::RuntimeEvent;
use orchest_runtime::model::{ModelAdapter, StreamEvent};
use orchest_runtime::run::{AgentConfig, AgentRun};
use orchest_runtime::tool::registry::ToolRegistry;
use orchest_runtime::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use orchest_provider::{create_adapter_from_config, ProviderRuntimeConfig};
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
        execution_mode: orchest_runtime::tool::ToolExecutionMode::Normal,
        parallelism: orchest_runtime::tool::ToolParallelism::Serial,
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
            model: "openrouter/anthropic/claude-sonnet-4".into(),
            api_key: None,
            api_key_env: Some("OPENROUTER_API_KEY".into()),
            api_url: None,
            max_tokens: Some(1024),
        })?);

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WeatherTool))?;

    let config = AgentConfig::builder("openrouter/anthropic/claude-sonnet-4")
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
