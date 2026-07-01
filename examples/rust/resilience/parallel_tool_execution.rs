//! Optional parallel tool execution example.
//!
//! Run with: cargo run --example parallel_tool_execution

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use orchest_runtime::events::RuntimeEvent;
use orchest_runtime::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest_runtime::run::{AgentConfig, AgentRun};
use orchest_runtime::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolMetadata, ToolOutput, ToolParallelism, ToolSource,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc;

struct TwoToolsModel {
    calls: AtomicU32,
}

impl TwoToolsModel {
    fn new() -> Self {
        Self {
            calls: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for TwoToolsModel {
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
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        if call == 0 {
            Ok(ModelResponse {
                content: vec![
                    ContentBlock::ToolUse {
                        id: "weather".into(),
                        name: "fetch_weather".into(),
                        input: json!({"city": "Shanghai"}),
                    },
                    ContentBlock::ToolUse {
                        id: "calendar".into(),
                        name: "fetch_calendar".into(),
                        input: json!({"day": "today"}),
                    },
                ],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Parallel lookup complete.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

struct LookupTool {
    name: &'static str,
    delay_ms: u64,
    metadata: ToolMetadata,
}

#[async_trait]
impl Tool for LookupTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "Read-only lookup that is safe to run in parallel"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }
    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        println!("[tool] {} finished", self.name);
        Ok(ToolOutput::Immediate(
            json!({"tool": self.name, "input": input}),
        ))
    }
}

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("Run independent read-only lookups.")
        .max_steps(4)
        .enable_parallel_tools()
        .build()
        .unwrap();

    let metadata = ToolMetadata {
        side_effect: false,
        approval: Approval::Never,
        parallelism: ToolParallelism::ParallelSafe,
        source: ToolSource::InProcess,
        ..ToolMetadata::default()
    };
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(LookupTool {
            name: "fetch_weather",
            delay_ms: 80,
            metadata: metadata.clone(),
        }))
        .unwrap();
    registry
        .register(Arc::new(LookupTool {
            name: "fetch_calendar",
            delay_ms: 80,
            metadata,
        }))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Fetch independent context.".into(),
        Arc::new(TwoToolsModel::new()),
        registry,
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ToolCallBatchStarted { batch_id, .. } => {
                println!("[batch] started {batch_id}")
            }
            RuntimeEvent::ToolCallBatchItemCompleted {
                tool,
                requested_order,
                completion_order,
                ..
            } => println!(
                "[batch] {tool} requested #{requested_order}, completed #{completion_order}"
            ),
            RuntimeEvent::RunCompleted { output } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
