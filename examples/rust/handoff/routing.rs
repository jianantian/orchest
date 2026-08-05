//! Example: Triage agent routing to Billing or Support via handoffs.
//!
//! The triage model calls the `route_to_billing` handoff tool on its first
//! turn.  The runtime switches to the billing AgentConfig, emits an
//! AgentUpdated event, and the same model then responds as the billing agent.
//!
//! Run with: cargo run --example handoff_routing

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::handoff::{Handoff, HandoffTarget};
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;
use serde_json::json;
use tokio::sync::mpsc;

// ── Shared model ──────────────────────────────────────────────────────────────
// Call 0 (triage): invoke the billing handoff tool.
// Call 1 (billing): reply as billing agent after the handoff.

struct RoutingModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for RoutingModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "routing"
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
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 8,
            output_tokens: 4,
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
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "route_to_billing".into(),
                    input: json!({"reason": "billing query"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(
                    "Hello! I'm the billing agent. How can I help?".into(),
                )],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let billing_config = AgentConfig::builder("billing", "mock/routing")
        .system_prompt("you are the billing department agent")
        .max_steps(2)
        .build()
        .unwrap();

    let support_config = AgentConfig::builder("support", "mock/routing")
        .system_prompt("you are the support department agent")
        .max_steps(2)
        .build()
        .unwrap();

    let triage_config = AgentConfig::builder("triage", "mock/routing")
        .system_prompt("you are a triage agent; route queries to billing or support")
        .max_steps(4)
        .build()
        .unwrap()
        .with_handoff(Handoff {
            tool_name: "route_to_billing".into(),
            tool_description: "Transfer the user to the billing department".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"reason": {"type": "string"}}
            }),
            target: HandoffTarget::Static(Box::new(billing_config)),
            input_filter: None,
            nest_history: false,
        })
        .with_handoff(Handoff {
            tool_name: "route_to_support".into(),
            tool_description: "Transfer the user to the support department".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"reason": {"type": "string"}}
            }),
            target: HandoffTarget::Static(Box::new(support_config)),
            input_filter: None,
            nest_history: false,
        });

    let model = Arc::new(RoutingModel {
        call_count: Default::default(),
    });

    let (handle, mut rx) = AgentRun::start(
        triage_config,
        "I have a billing question".into(),
        model,
        ToolRegistry::new(),
    );

    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::AgentUpdated {
                previous_agent,
                new_agent,
            } => {
                println!("[event] AgentUpdated: '{previous_agent}' → '{new_agent}'");
            }
            RuntimeEvent::RunCompleted { output, .. } => println!("[event] RunCompleted: {output}"),
            RuntimeEvent::RunFailed { error, .. } => println!("[event] RunFailed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
