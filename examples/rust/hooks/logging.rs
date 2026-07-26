//! Example: custom Hook that logs before_model, after_tool, and repeated failures.
//!
//! Run with: cargo run --example hook_logging

use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::hook::{
    Hook, HookAction, ModelHookAction, ModelHookContext, RepeatedFailureHookContext,
    ToolHookContext,
};
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct MockModel;

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
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
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
            content: vec![ContentBlock::Text("task complete".into())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Logging hook ──────────────────────────────────────────────────────────────

struct LoggingHook;

#[async_trait]
impl Hook for LoggingHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        println!(
            "[hook] before_model: step has {} messages, model={}",
            ctx.messages.len(),
            ctx.model_spec.model
        );
        ModelHookAction::Continue
    }

    async fn after_tool(&self, ctx: &mut ToolHookContext) -> HookAction {
        println!(
            "[hook] after_tool: tool='{}' input={}",
            ctx.tool_name, ctx.tool_input
        );
        HookAction::Continue
    }

    async fn on_repeated_failure(&self, ctx: &RepeatedFailureHookContext) -> HookAction {
        println!(
            "[hook] repeated failure: tool='{}' kind={:?} count={} history={}",
            ctx.tool_name,
            ctx.error_kind,
            ctx.count,
            ctx.error_history.len()
        );
        HookAction::Continue
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("you are a helpful assistant")
        .max_steps(3)
        .repeated_failure_threshold(3)
        .build()
        .unwrap()
        .with_hook(Arc::new(LoggingHook));

    let (handle, mut rx) = AgentRun::start(
        config,
        "hello".into(),
        Arc::new(MockModel),
        ToolRegistry::new(),
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output, .. } => {
                println!("[run] completed: {output}");
            }
            RuntimeEvent::RunFailed { error } => {
                println!("[run] failed: {error}");
            }
            _ => {}
        }
    }
    handle.wait().await;
}
