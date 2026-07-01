//! Basic agent run — the smallest possible Orchest agent.
//!
//! Configures an Anthropic provider, registers one read-only tool, starts a
//! run, streams the model's output, and waits for completion.
//!
//! Run with:
//!   ANTHROPIC_API_KEY=sk-... cargo run --example basic_agent_run
//!
//! This example focuses on the minimal happy path: no hooks, guardrails,
//! sessions, handoffs, or watchers. See the other files in `examples/rust/`
//! for those advanced scenarios.

use std::sync::{Arc, LazyLock};

use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::{ModelAdapter, StreamEvent};
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::json;

// ── A minimal, read-only tool ───────────────────────────────────────────────
//
// A tool is any type implementing the `Tool` trait. `get_current_time` takes no
// arguments and has no side effects, so its approval level is `Never`.

struct CurrentTimeTool;

/// `input_schema()` / `metadata()` return references, so the values must live
/// for `'static`. `LazyLock` bundles the value and initializer together.
fn empty_object_schema() -> &'static JsonSchema {
    static SCHEMA: LazyLock<JsonSchema> =
        LazyLock::new(|| json!({ "type": "object", "properties": {} }));
    &SCHEMA
}

fn time_tool_metadata() -> &'static ToolMetadata {
    static META: LazyLock<ToolMetadata> = LazyLock::new(|| ToolMetadata {
        side_effect: false,
        approval: Approval::Never,
        execution_mode: agent_runtime_core::tool::ToolExecutionMode::Normal,
        parallelism: agent_runtime_core::tool::ToolParallelism::Serial,
        cost_hint: None,
        timeout: None,
        max_output_tokens: None,
        source: ToolSource::InProcess,
    });
    &META
}

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str {
        "get_current_time"
    }

    fn description(&self) -> &str {
        "Return the current UTC time as an ISO-8601 string."
    }

    fn input_schema(&self) -> &JsonSchema {
        empty_object_schema()
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        time_tool_metadata()
    }

    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        // A real implementation would use `chrono`/`time`; for a starter example
        // a fixed value keeps the dependency footprint at zero.
        Ok(ToolOutput::Immediate(
            json!({ "utc": "2026-06-06T00:00:00Z" }),
        ))
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Configure a provider. The model string is `provider/model`; the API key
    //    is read from the ANTHROPIC_API_KEY environment variable.
    let model: Arc<dyn ModelAdapter> = Arc::from(orchest_providers::create_adapter_from_config(
        orchest_providers::ProviderRuntimeConfig {
            model: "anthropic/claude-sonnet-4-6".into(),
            api_key: None,
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            api_url: None,
            max_tokens: Some(1024),
        },
    )?);

    // 2. Register the tool into a registry.
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CurrentTimeTool))?;

    // 3. Build the run configuration.
    let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
        .system_prompt("You are a helpful assistant. Use tools when useful.")
        .max_steps(5)
        .build()?;

    // 4. Start the run. `start` returns a handle plus the event receiver.
    let (handle, mut rx) = AgentRun::start(config, "What time is it?".into(), model, registry);

    // 5. Consume events as they stream in, then wait for the run to finish.
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
    handle.wait().await;
    Ok(())
}
