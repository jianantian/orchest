//! Example: LoopDetectionHook — warning injection then run abort.
//!
//! A model repeatedly calls a "search" tool with identical arguments.
//! With warn_threshold=2 the hook injects a warning into the next model
//! context; at stop_threshold=3 it aborts the run with RunFailed.
//!
//! Run with: cargo run --example loop_detection

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use orchest_runtime::events::RuntimeEvent;
use orchest_runtime::hook::LoopDetectionConfig;
use orchest_runtime::model::{
    ContentBlock, JsonSchema, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest_runtime::run::{AgentConfig, AgentRun};
use orchest_runtime::tool::registry::ToolRegistry;
use orchest_runtime::tool::{
    Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ── Echo search tool ──────────────────────────────────────────────────────────

struct SearchTool;

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "search the web"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
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
            source: ToolSource::Builtin,
        }
    }
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(json!({"result": "no results found"})))
    }
}

// ── Looping model ─────────────────────────────────────────────────────────────
// Always calls "search" with the same arguments, provoking the loop detector.

struct LoopingModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for LoopingModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "looping"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest_runtime::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 2,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::ToolUse {
                id: format!("call_{n}"),
                name: "search".into(),
                input: json!({"q": "rust loops"}),
            }],
            usage,
            stop_reason: StopReason::ToolUse,
            option_adjustments: vec![],
        })
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/looping")
        .system_prompt("assistant")
        .max_steps(10)
        .build()
        .unwrap()
        .with_loop_detection_config(LoopDetectionConfig {
            window_size: 10,
            warn_threshold: 2,
            stop_threshold: 3,
        });

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(SearchTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "search for something".into(),
        Arc::new(LoopingModel {
            call_count: Default::default(),
        }),
        registry,
    );

    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::RunCompleted { output } => println!("[event] RunCompleted: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[event] RunFailed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
