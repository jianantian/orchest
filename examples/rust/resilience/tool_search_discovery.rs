//! Deferred tool discovery example for large registries.
//!
//! Run with: cargo run --example tool_search_discovery

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

struct DiscoverThenCallModel {
    calls: AtomicU32,
}

impl DiscoverThenCallModel {
    fn new() -> Self {
        Self {
            calls: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for DiscoverThenCallModel {
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
        tools: &[ToolDef],
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

        let (content, stop_reason) = match call {
            0 => {
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].name, "search_tools");
                (
                    vec![ContentBlock::ToolUse {
                        id: "search_1".into(),
                        name: "search_tools".into(),
                        input: json!({"query": "email delivery", "top_k": 3}),
                    }],
                    StopReason::ToolUse,
                )
            }
            1 => {
                assert!(tools.iter().any(|tool| tool.name == "send_email_17"));
                (
                    vec![ContentBlock::ToolUse {
                        id: "send_1".into(),
                        name: "send_email_17".into(),
                        input: json!({"to": "ops@example.com", "subject": "Discovery worked"}),
                    }],
                    StopReason::ToolUse,
                )
            }
            _ => (
                vec![ContentBlock::Text(
                    "Discovered and called send_email_17.".into(),
                )],
                StopReason::EndTurn,
            ),
        };

        Ok(ModelResponse {
            content,
            usage,
            stop_reason,
            option_adjustments: vec![],
        })
    }
}

struct CatalogTool {
    name: String,
    description: String,
    schema: JsonSchema,
    metadata: ToolMetadata,
}

#[async_trait]
impl Tool for CatalogTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn input_schema(&self) -> &JsonSchema {
        &self.schema
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        println!("[tool] {} called with {input}", self.name);
        Ok(ToolOutput::Immediate(
            json!({"tool": self.name, "ok": true}),
        ))
    }
}

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("Search the tool catalog before calling tools.")
        .max_steps(5)
        .enable_tool_search()
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    for idx in 0..30 {
        let is_email_tool = idx == 17;
        registry
            .register(Arc::new(CatalogTool {
                name: if is_email_tool {
                    "send_email_17".into()
                } else {
                    format!("catalog_tool_{idx}")
                },
                description: if is_email_tool {
                    "Send an email delivery notification".into()
                } else {
                    format!("General catalog utility {idx}")
                },
                schema: json!({"type": "object"}),
                metadata: ToolMetadata {
                    side_effect: false,
                    approval: Approval::Never,
                    source: ToolSource::InProcess,
                    ..ToolMetadata::default()
                },
            }))
            .unwrap();
    }

    let (handle, mut rx) = AgentRun::start(
        config,
        "Notify ops after discovering the right tool.".into(),
        Arc::new(DiscoverThenCallModel::new()),
        registry,
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ToolCallStarted { tool, .. } => println!("[run] started {tool}"),
            RuntimeEvent::RunCompleted { output } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
