//! Watcher abort example: abort the run when ToolCallFailed exceeds a threshold.
//!
//! Run with: cargo run --example watcher_abort_on_pattern

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use agent_runtime_core::run::{AgentConfig, AgentRun, Watcher, WatcherAction};
use agent_runtime_core::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct AlwaysFailToolModel {
    call: AtomicU32,
}

impl AlwaysFailToolModel {
    fn new() -> Self {
        Self {
            call: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for AlwaysFailToolModel {
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
        let count = self.call.fetch_add(1, Ordering::SeqCst);
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

        // Keep calling the failing tool unless a ToolResult message exists
        let has_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if !has_result || count < 5 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: format!("call_{count}"),
                    name: "flaky_tool".into(),
                    input: json!({"attempt": count}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
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

// ── Flaky tool (always errors) ────────────────────────────────────────────────

struct FlakyTool;

#[async_trait]
impl Tool for FlakyTool {
    fn name(&self) -> &str {
        "flaky_tool"
    }
    fn description(&self) -> &str {
        "A tool that always fails"
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
        Err(ToolError::fatal("tool unavailable"))
    }
}

// ── Watcher ───────────────────────────────────────────────────────────────────

struct AbortOnRepeatedFailure {
    threshold: u32,
    failures: AtomicU32,
}

impl AbortOnRepeatedFailure {
    fn new(threshold: u32) -> Self {
        Self {
            threshold,
            failures: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl Watcher for AbortOnRepeatedFailure {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if let RuntimeEvent::ToolCallFailed { tool, error } = event {
            let count = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
            println!(
                "[watcher] tool '{tool}' failed ({count}/{}) — {error}",
                self.threshold
            );
            if count >= self.threshold {
                println!("[watcher] threshold reached — aborting run");
                return WatcherAction::Abort(format!("too many tool failures ({count}): aborting"));
            }
        }
        WatcherAction::Continue
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("You are a resilient assistant.")
        .max_steps(20)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FlakyTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Use the flaky tool.".into(),
        Arc::new(AlwaysFailToolModel::new()),
        registry,
    );

    handle
        .attach_watcher(Arc::new(AbortOnRepeatedFailure::new(3)), 1024)
        .await;

    let mut aborted = false;
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            RuntimeEvent::RunAborted { reason } => {
                println!(
                    "[run] aborted: {}",
                    reason.as_deref().unwrap_or("no reason")
                );
                aborted = true;
            }
            RuntimeEvent::ToolCallFailed { tool, error } => {
                println!("[run] tool '{tool}' failed: {error}")
            }
            _ => {}
        }
    }
    handle.wait().await;

    assert!(aborted, "run should have been aborted by the watcher");
    println!("Done. Watcher aborted the run after repeated tool failures.");
}
