//! ToolInputGuardrail example: block tool calls containing banned keywords.
//!
//! Run with: cargo run --example guardrail_keyword_filter

use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::guardrail::{ToolInputGuardrail, ToolInputGuardrailAction};
use orchest::hook::ToolHookContext;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolMetadata, ToolOutput, ToolSource,
};
use serde_json::json;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct MockModel {
    call: std::sync::atomic::AtomicU32,
}

impl MockModel {
    fn new() -> Self {
        Self {
            call: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for MockModel {
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

        // If there's a ToolResult (after the rejected call), end the run
        let has_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if count == 0 {
            // First call: request a banned tool call
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "search".into(),
                    input: json!({"query": "drop table users"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else if count == 1 && !has_result {
            // Second attempt without banned word
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_2".into(),
                    name: "search".into(),
                    input: json!({"query": "safe query"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Search complete.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

// ── Guardrail ─────────────────────────────────────────────────────────────────

struct KeywordBlockGuardrail {
    banned: Vec<String>,
}

#[async_trait]
impl ToolInputGuardrail for KeywordBlockGuardrail {
    async fn check(&self, ctx: &ToolHookContext) -> ToolInputGuardrailAction {
        let input_str = ctx.tool_input.to_string().to_lowercase();
        for word in &self.banned {
            if input_str.contains(word.as_str()) {
                println!(
                    "[guardrail] REJECT: tool='{}' contains banned keyword '{word}'",
                    ctx.tool_name
                );
                return ToolInputGuardrailAction::Reject(format!(
                    "blocked: input contains banned keyword '{word}'"
                ));
            }
        }
        println!("[guardrail] ALLOW: tool='{}'", ctx.tool_name);
        ToolInputGuardrailAction::Allow
    }
}

// ── Fake search tool ──────────────────────────────────────────────────────────

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
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        println!("[tool] search called with: {input}");
        Ok(ToolOutput::Immediate(
            json!({"results": ["result1", "result2"]}),
        ))
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("You are a helpful assistant.")
        .max_steps(5)
        .build()
        .unwrap()
        .with_tool_input_guardrail(Arc::new(KeywordBlockGuardrail {
            banned: vec!["drop".into(), "delete".into(), "truncate".into()],
        }));

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(SearchTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Search for something.".into(),
        Arc::new(MockModel::new()),
        registry,
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output, .. } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            RuntimeEvent::ToolCallFailed { tool, error } => {
                println!("[run] tool '{tool}' failed: {error}")
            }
            _ => {}
        }
    }
    handle.wait().await;
    println!("Done.");
}
