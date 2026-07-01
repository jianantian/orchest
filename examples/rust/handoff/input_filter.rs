//! Example: Handoff with a custom HandoffInputFilter.
//!
//! A `LastMessageFilter` trims the history to just the system message and
//! the final user turn before the specialist agent receives it.  The
//! specialist model reports how many messages it received so the trimming
//! is visible in the output.
//!
//! Run with: cargo run --example handoff_input_filter

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::handoff::{
    Handoff, HandoffError, HandoffInputData, HandoffInputFilter, HandoffTarget,
};
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;
use serde_json::json;
use tokio::sync::mpsc;

// ── Input filter ──────────────────────────────────────────────────────────────

struct LastMessageFilter;

#[async_trait]
impl HandoffInputFilter for LastMessageFilter {
    async fn filter(&self, mut data: HandoffInputData) -> Result<HandoffInputData, HandoffError> {
        let system: Vec<Message> = data
            .history
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .cloned()
            .collect();
        let last_user = data
            .history
            .into_iter()
            .rfind(|m| !matches!(m.role, Role::System));
        data.history = system;
        if let Some(msg) = last_user {
            data.history.push(msg);
        }
        println!(
            "[filter] trimmed history to {} messages",
            data.history.len()
        );
        Ok(data)
    }
}

// ── Shared model ──────────────────────────────────────────────────────────────
// Call 0: invoke the escalation handoff tool.
// Call 1: specialist responds, reporting the message count it received.

struct HandoffModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for HandoffModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "handoff"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 6,
            output_tokens: 3,
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
                    name: "escalate_to_specialist".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            let msg_count = messages.len();
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(format!(
                    "specialist here — received {msg_count} messages after filter"
                ))],
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
    let specialist_config = AgentConfig::builder("mock/handoff")
        .system_prompt("you are a specialist")
        .max_steps(2)
        .build()
        .unwrap();

    let triage_config = AgentConfig::builder("mock/handoff")
        .system_prompt("you are a triage agent")
        .max_steps(4)
        .build()
        .unwrap()
        .with_handoff(Handoff {
            tool_name: "escalate_to_specialist".into(),
            tool_description: "Escalate to a specialist, sending only the latest message".into(),
            input_schema: json!({"type": "object", "properties": {}}),
            target: HandoffTarget::Static(Box::new(specialist_config)),
            input_filter: Some(Arc::new(LastMessageFilter)),
            nest_history: false,
        });

    let model = Arc::new(HandoffModel {
        call_count: Default::default(),
    });

    let (handle, mut rx) = AgentRun::start(
        triage_config,
        "please help me".into(),
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
            RuntimeEvent::RunCompleted { output } => println!("[event] RunCompleted: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[event] RunFailed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
}
