//! Explicit code execution executor example.
//!
//! Run with: cargo run -p orchest-runtime --example code_execution_executor

use std::sync::Arc;

use orchest_runtime::events::RuntimeEvent;
use orchest_runtime::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest_runtime::run::{AgentConfig, AgentRun};
use orchest_runtime::skill::executor::BareSubprocessExecutor;
use orchest_runtime::tool::registry::ToolRegistry;
use serde_json::json;
use tokio::sync::mpsc;

struct CodeExecModel;

#[async_trait::async_trait]
impl ModelAdapter for CodeExecModel {
    fn provider_name(&self) -> &str {
        "example"
    }

    fn model_name(&self) -> &str {
        "code-exec-demo"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[orchest_runtime::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            ..Default::default()
        };
        let has_tool_result = messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|block| matches!(block, ContentBlock::ToolResult { .. }));

        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "code-1".into(),
                    name: "execute_python".into(),
                    input: json!({
                        "code": "print(sum([1, 2, 3]))",
                        "timeout_seconds": 5
                    }),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("example/code-exec-demo")
        .system_prompt("Run trusted demo code.")
        .code_execution_executor(Arc::new(BareSubprocessExecutor::new()))
        .max_steps(4)
        .build()
        .expect("valid config");

    let (_handle, mut events) = AgentRun::start(
        config,
        "calculate a tiny sum".into(),
        Arc::new(CodeExecModel),
        ToolRegistry::new(),
    );

    while let Some(event) = events.recv().await {
        match event {
            RuntimeEvent::ToolCallCompleted { tool, output, .. } => {
                println!("{tool}: {output}");
            }
            RuntimeEvent::RunCompleted { output, .. } => {
                println!("{output}");
            }
            _ => {}
        }
    }
}
