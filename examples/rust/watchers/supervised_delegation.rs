//! Supervised delegation: end-to-end example showing LlmWatcher, steering, and crash recovery.
//!
//! Run with: cargo run --example supervised_delegation

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun, SupervisionStrategy, Watcher, WatcherAction};
use orchest::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata,
    ToolOutput, ToolSource,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

struct ScriptedModel {
    call: AtomicU32,
}

#[async_trait]
impl ModelAdapter for ScriptedModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "mock"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        match n {
            0 => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "long_task".into(),
                    input: json!({"duration": 100}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            _ => Ok(ModelResponse {
                content: vec![ContentBlock::Text("Task completed.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
        }
    }
}

struct LongTask;

use orchest::tool::ToolDef;

#[async_trait]
impl Tool for LongTask {
    fn name(&self) -> &str {
        "long_task"
    }
    fn description(&self) -> &str {
        "Simulates a long-running task"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            source: ToolSource::Builtin,
            side_effect: false,
            timeout: None,
            max_output_tokens: None,
            approval: Approval::Never,
            execution_mode: orchest::tool::ToolExecutionMode::Normal,
            parallelism: orchest::tool::ToolParallelism::Serial,
            cost_hint: None,
        }
    }
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Ok(ToolOutput::Immediate(json!({"status": "done"})))
    }
}

struct SteeringWatcher;

#[async_trait]
impl Watcher for SteeringWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        match event {
            RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "long_task" => {
                WatcherAction::Steer("Focus on summarizing results.".to_string())
            }
            _ => WatcherAction::Continue,
        }
    }
}

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("supervised-agent", "mock")
        .system_prompt("You are a research assistant.")
        .max_steps(5)
        .supervision_strategy(SupervisionStrategy::Restart { max_retries: 2 })
        .build()
        .unwrap();

    let model: Arc<dyn ModelAdapter> = Arc::new(ScriptedModel {
        call: AtomicU32::new(0),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(LongTask)).unwrap();

    let (handle, mut rx) =
        AgentRun::start(config, "Research quantum computing".into(), model, registry);

    let watcher: Arc<dyn Watcher> = Arc::new(SteeringWatcher);
    handle.attach_watcher(watcher, 256).await;

    let event_task = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            let label = match &event {
                RuntimeEvent::RunStarted { .. } => "RunStarted".to_string(),
                RuntimeEvent::ModelCallStarted { step } => {
                    format!("ModelCallStarted(step={step})")
                }
                RuntimeEvent::ToolCallStarted { tool, .. } => format!("ToolCallStarted({tool})"),
                RuntimeEvent::ToolCallCompleted { tool, duration, .. } => {
                    format!("ToolCallCompleted({}, {}ms)", tool, duration.as_millis())
                }
                RuntimeEvent::RunCompleted { .. } => "RunCompleted".to_string(),
                RuntimeEvent::RunAborted { reason } => {
                    format!("RunAborted({reason:?})")
                }
                RuntimeEvent::RunRestarted { attempt } => {
                    format!("RunRestarted(attempt={attempt})")
                }
                _ => format!("{event:?}").chars().take(60).collect(),
            };
            println!("  [event] {label}");
            events.push(event);
        }
        events
    });

    handle.wait().await;

    let events = event_task.await.unwrap();
    println!("\nTotal events: {}", events.len());
    println!(
        "Run completed: {}",
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. }))
    );
}
