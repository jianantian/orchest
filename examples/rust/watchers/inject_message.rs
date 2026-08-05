//! Watcher example: attach_watcher monitors ToolCallCompleted and injects a steering message.
//!
//! Run with: cargo run --example watcher_inject_message

use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun, Watcher, WatcherAction};
use orchest::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolMetadata, ToolOutput, ToolSource,
};
use serde_json::json;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct ToolThenEndModel {
    call: std::sync::atomic::AtomicU32,
}

impl ToolThenEndModel {
    fn new() -> Self {
        Self {
            call: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for ToolThenEndModel {
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

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if count == 0 {
            // First call: use a tool
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "query".into(),
                    input: json!({"q": "status"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else if has_tool_result {
            // After tool result (possibly with injected message): finish
            let injected = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::Text(t) if t.contains("injected")))
            });
            if injected {
                println!("[model] received injected message from watcher");
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("All done.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Done.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

// ── Tool ──────────────────────────────────────────────────────────────────────

struct QueryTool;

#[async_trait]
impl Tool for QueryTool {
    fn name(&self) -> &str {
        "query"
    }
    fn description(&self) -> &str {
        "query data"
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
            execution_mode: orchest::tool::ToolExecutionMode::Normal,
            parallelism: orchest::tool::ToolParallelism::Serial,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(json!({"status": "ok"})))
    }
}

// ── Watcher ───────────────────────────────────────────────────────────────────

struct InjectOnToolComplete;

#[async_trait]
impl Watcher for InjectOnToolComplete {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if let RuntimeEvent::ToolCallCompleted { tool, .. } = event {
            println!("[watcher] tool '{tool}' completed — injecting steering message");
            return WatcherAction::Inject(
                "[injected by watcher] Please summarize what was found.".into(),
            );
        }
        WatcherAction::Continue
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("watched-agent", "mock/mock")
        .system_prompt("You are a data analyst.")
        .max_steps(5)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(QueryTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Query the status.".into(),
        Arc::new(ToolThenEndModel::new()),
        registry,
    );

    handle
        .attach_watcher(Arc::new(InjectOnToolComplete), 1024)
        .await;

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output, .. } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error, .. } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
    println!("Done.");
}
