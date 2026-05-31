//! Example: before_model hook that injects context into the message list.
//!
//! Run with: cargo run --example hook_modifier

use std::sync::Arc;

use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::hook::{Hook, ModelHookAction, ModelHookContext};
use agent_runtime_core::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
};
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::registry::ToolRegistry;
use async_trait::async_trait;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct EchoModel;

#[async_trait]
impl ModelAdapter for EchoModel {
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
        _tools: &[agent_runtime_core::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        // Find the injected context message to demonstrate it arrived
        let injected = messages
            .iter()
            .find(|m| {
                m.content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Text(t) if t.contains("[injected]")))
            })
            .is_some();

        let reply = if injected {
            "received injected context".to_string()
        } else {
            "no injection".to_string()
        };

        let usage = TokenUsage {
            input_tokens: 8,
            output_tokens: 4,
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
            content: vec![ContentBlock::Text(reply)],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Modifier hook ─────────────────────────────────────────────────────────────

struct ContextInjectorHook {
    context: String,
}

#[async_trait]
impl Hook for ContextInjectorHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        // Append injected context to the system message to avoid consecutive same-role messages
        if let Some(system_msg) = ctx.messages.first_mut() {
            if matches!(system_msg.role, Role::System) {
                system_msg
                    .content
                    .push(ContentBlock::Text(format!("[injected] {}", self.context)));
            }
        }
        println!("[hook] injected context into message list");
        ModelHookAction::Continue
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("assistant")
        .max_steps(2)
        .build()
        .unwrap()
        .with_hook(Arc::new(ContextInjectorHook {
            context: "today's date is 2026-05-30".into(),
        }));

    let (handle, mut rx) = AgentRun::start(
        config,
        "what is today's date?".into(),
        Arc::new(EchoModel),
        ToolRegistry::new(),
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
