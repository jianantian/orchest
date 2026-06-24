//! Draft/Commit tool pair example: draft creates a side-effect-free plan,
//! commit applies that plan and requires approval by default.
//!
//! Run with: cargo run --example draft_commit_approval

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use agent_runtime_core::events::{ApprovalContext, RuntimeEvent};
use agent_runtime_core::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use agent_runtime_core::run::{AgentConfig, AgentRun, ApprovalMode};
use agent_runtime_core::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolExecutionMode, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

struct DraftThenCommitModel {
    calls: AtomicU32,
}

impl DraftThenCommitModel {
    fn new() -> Self {
        Self {
            calls: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for DraftThenCommitModel {
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

        let content = match call {
            0 => vec![ContentBlock::ToolUse {
                id: "draft_1".into(),
                name: "draft_file_write".into(),
                input: json!({"path": "/tmp/report.md", "content": "Quarterly draft"}),
            }],
            1 => vec![ContentBlock::ToolUse {
                id: "commit_1".into(),
                name: "commit_file_write".into(),
                input: latest_tool_result(messages).unwrap_or_else(|| json!({})),
            }],
            _ => vec![ContentBlock::Text("File write completed.".into())],
        };
        let stop_reason = if call < 2 {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        };

        Ok(ModelResponse {
            content,
            usage,
            stop_reason,
            option_adjustments: vec![],
        })
    }
}

fn latest_tool_result(messages: &[Message]) -> Option<Value> {
    messages.iter().rev().find_map(|message| {
        message.content.iter().rev().find_map(|block| match block {
            ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
    })
}

struct FileTool {
    name: &'static str,
    metadata: ToolMetadata,
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "Draft or commit a file write"
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
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let output = if self.name == "draft_file_write" {
            println!("[draft] planned write: {input}");
            json!({"path": input["path"], "content": input["content"], "approved_plan_id": "plan_123"})
        } else {
            println!("[commit] applied draft output: {input}");
            json!({"written": true, "path": input["path"]})
        };
        Ok(ToolOutput::Immediate(output))
    }
}

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("Create a draft plan before committing file writes.")
        .max_steps(5)
        .approval_mode(ApprovalMode::None)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(FileTool {
            name: "draft_file_write",
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Always,
                execution_mode: ToolExecutionMode::Draft {
                    commit_tool: "commit_file_write".into(),
                },
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
        }))
        .unwrap();
    registry
        .register(Arc::new(FileTool {
            name: "commit_file_write",
            metadata: ToolMetadata {
                side_effect: true,
                approval: Approval::Never,
                execution_mode: ToolExecutionMode::Commit {
                    draft_tool: "draft_file_write".into(),
                },
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
        }))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Write /tmp/report.md after drafting the change.".into(),
        Arc::new(DraftThenCommitModel::new()),
        registry,
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ApprovalRequested { tool_call, context } => {
                if let ApprovalContext::CommitToolCall { draft_tool } = context {
                    println!(
                        "[approval] '{}' commits draft output from '{}'",
                        tool_call.name, draft_tool
                    );
                }
                handle.respond_approval(handle.run_id, true).await.unwrap();
            }
            RuntimeEvent::RunCompleted { output } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
