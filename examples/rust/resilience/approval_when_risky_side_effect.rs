//! Approval::WhenRisky example: only side-effecting risky tools require approval.
//!
//! Run with: cargo run --example approval_when_risky_side_effect

use std::sync::Arc;

use orchest_runtime::events::RuntimeEvent;
use orchest_runtime::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest_runtime::run::{AgentConfig, AgentRun};
use orchest_runtime::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct TwoToolModel {
    call: std::sync::atomic::AtomicU32,
}

impl TwoToolModel {
    fn new() -> Self {
        Self {
            call: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for TwoToolModel {
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
        messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let count = self.call.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

        let tool_results = messages
            .iter()
            .filter(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            })
            .count();

        match (count, tool_results) {
            (0, _) => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_read".into(),
                    name: "read_file".into(),
                    input: json!({"path": "/tmp/data.txt"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            (1, _) => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_write".into(),
                    name: "write_file".into(),
                    input: json!({"path": "/tmp/out.txt", "content": "hello"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            _ => Ok(ModelResponse {
                content: vec![ContentBlock::Text("All done.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
        }
    }
}

// ── Tools ─────────────────────────────────────────────────────────────────────

struct ReadFileTool;
struct WriteFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read a file (no side effects)"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: false,
            approval: Approval::Never,
            execution_mode: orchest_runtime::tool::ToolExecutionMode::Normal,
            parallelism: orchest_runtime::tool::ToolParallelism::Serial,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        println!("[tool] read_file: {input} (no approval needed)");
        Ok(ToolOutput::Immediate(
            json!({"content": "file contents here"}),
        ))
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "Write a file (has side effects)"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: true,
            approval: Approval::WhenRisky,
            execution_mode: orchest_runtime::tool::ToolExecutionMode::Normal,
            parallelism: orchest_runtime::tool::ToolParallelism::Serial,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        println!("[tool] write_file: {input} (approved!)");
        Ok(ToolOutput::Immediate(json!({"written": true})))
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("You are a file assistant.")
        .max_steps(5)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadFileTool)).unwrap();
    registry.register(Arc::new(WriteFileTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Process some files.".into(),
        Arc::new(TwoToolModel::new()),
        registry,
    );

    let mut saw_approval = false;
    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::ApprovalRequested { tool_call, context } => {
                println!(
                    "[approval] requested for tool '{}' ({context:?})",
                    tool_call.name
                );
                saw_approval = true;
                handle.respond_approval(handle.run_id, true).await.unwrap();
            }
            RuntimeEvent::RunCompleted { output } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;

    assert!(
        saw_approval,
        "write_file (side_effect=true) should require approval"
    );
    println!("Done. write_file required approval; read_file did not.");
}
