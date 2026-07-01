//! Runtime metrics recorder example.
//!
//! Run with: cargo run -p orchest --example observability_metrics

use std::sync::Arc;

use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;
use metrics_util::debugging::DebuggingRecorder;
use tokio::sync::mpsc;

struct FinalModel;

#[async_trait::async_trait]
impl ModelAdapter for FinalModel {
    fn provider_name(&self) -> &str {
        "example"
    }

    fn model_name(&self) -> &str {
        "gpt-demo"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("ok".into())],
            usage: TokenUsage {
                input_tokens: 8,
                output_tokens: 3,
                ..Default::default()
            },
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

#[tokio::main]
async fn main() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    recorder.install().expect("install metrics recorder");

    let config = AgentConfig::builder("example/gpt-demo")
        .system_prompt("Answer tersely.")
        .max_tokens(100)
        .build()
        .expect("valid config");

    let (handle, mut events) = AgentRun::start(
        config,
        "ping".into(),
        Arc::new(FinalModel),
        ToolRegistry::new(),
    );
    while events.recv().await.is_some() {}
    handle.wait().await;

    let mut names: Vec<_> = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .map(|(key, _, _, _)| key.key().name().to_string())
        .collect();
    names.sort();
    names.dedup();
    for name in names {
        println!("{name}");
    }
}
