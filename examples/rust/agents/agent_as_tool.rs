//! Example: AgentConfig::as_tool() — run a child agent as a tool.
//!
//! A parent agent calls a "summariser" child-agent tool. The child agent
//! receives a prompt and returns a summary. The parent sees the result.
//!
//! Run with: cargo run --example agent_as_tool

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::agent_as_tool::ContextMode;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::ToolError;
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ── Parent model ──────────────────────────────────────────────────────────────
// First call: invoke the summariser tool.
// Second call: end turn using the tool result.

struct ParentModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for ParentModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "parent"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
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
        if n == 0 {
            // Call child agent tool
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "summariser".into(),
                    input: json!({"input": "long text to summarise"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            // Use tool result
            let tool_result = messages.iter().find_map(|m| {
                m.content.iter().find_map(|b| match b {
                    ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                    _ => None,
                })
            });
            let text = tool_result
                .as_ref()
                .and_then(|v| v.get("output").and_then(Value::as_str))
                .unwrap_or("(no summary)")
                .to_string();
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(format!("parent done, summary: {text}"))],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

// ── Child model ───────────────────────────────────────────────────────────────

struct ChildModel;

#[async_trait]
impl ModelAdapter for ChildModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "child"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        };
        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("concise summary".into())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let child_model: Arc<dyn ModelAdapter> = Arc::new(ChildModel);
    let child_registry = ToolRegistry::new();

    let child_config = AgentConfig::builder("child", "mock/child")
        .system_prompt("you are a summariser")
        .max_steps(2)
        .build()
        .unwrap();

    // Wrap child agent as a Tool the parent can call
    let summariser_tool = child_config
        .as_tool("summariser", "Summarises long text")
        .model(Arc::clone(&child_model))
        .registry(child_registry)
        .context_mode(ContextMode::Fresh)
        .input_schema(json!({
            "type": "object",
            "properties": {"input": {"type": "string"}},
            "required": ["input"]
        }))
        .input_mapper(|input: Value| {
            input
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ToolError::fatal("missing input field"))
        })
        .output_extractor(|details: Value| {
            json!({"output": details.get("output").cloned().unwrap_or_else(|| details.clone())})
        })
        .build()
        .unwrap();

    let parent_config = AgentConfig::builder("parent", "mock/parent")
        .system_prompt("you are a research assistant")
        .max_steps(3)
        .build()
        .unwrap();

    let mut parent_registry = ToolRegistry::new();
    parent_registry.register(summariser_tool).unwrap();

    let (handle, mut rx) = AgentRun::start(
        parent_config,
        "summarise this document".into(),
        Arc::new(ParentModel {
            call_count: Default::default(),
        }),
        parent_registry,
    );

    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::SubAgentStarted { child_run_id, .. } => {
                println!("[event] SubAgentStarted child={child_run_id}");
            }
            RuntimeEvent::SubAgentCompleted {
                child_run_id,
                output,
                ..
            } => {
                println!("[event] SubAgentCompleted child={child_run_id} output={output}");
            }
            RuntimeEvent::RunCompleted { output, .. } => println!("[event] RunCompleted: {output}"),
            RuntimeEvent::RunFailed { error, .. } => println!("[event] RunFailed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
