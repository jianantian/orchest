//! Example: hook that aborts the run, and hook panic recovery.
//!
//! Demonstrates two error-path scenarios:
//!   1. A panicking hook — the run emits HookPanicked and continues.
//!   2. A hook that returns Abort — the run emits RunFailed.
//!
//! Run with: cargo run --example hook_abort

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::hook::{Hook, ModelHookAction, ModelHookContext, RunHookContext};
use agent_runtime_core::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::registry::ToolRegistry;
use async_trait::async_trait;
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
        _tools: &[agent_runtime_core::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
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
            content: vec![ContentBlock::Text("ok".into())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Panicking hook ────────────────────────────────────────────────────────────

struct PanickingHook;

#[async_trait]
impl Hook for PanickingHook {
    async fn on_run_start(&self, _ctx: &mut RunHookContext) {
        panic!("deliberate panic in hook — should be caught by runner");
    }
}

// ── Aborting hook ─────────────────────────────────────────────────────────────

struct AbortAfterNHook {
    max_calls: u32,
    calls: AtomicU32,
}

impl AbortAfterNHook {
    fn new(max_calls: u32) -> Self {
        Self {
            max_calls,
            calls: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl Hook for AbortAfterNHook {
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n >= self.max_calls {
            println!("[hook] aborting run at call {n}");
            ModelHookAction::Abort(format!("hook aborted after {n} calls"))
        } else {
            ModelHookAction::Continue
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Scenario A: panicking hook (run still terminates, emits HookPanicked)
    println!("=== Scenario A: panicking hook ===");
    let config_a = AgentConfig::builder("mock/mock")
        .system_prompt("assistant")
        .max_steps(2)
        .build()
        .unwrap()
        .with_hook(Arc::new(PanickingHook));

    let (handle_a, mut rx_a) = AgentRun::start(
        config_a,
        "go".into(),
        Arc::new(MockModel),
        ToolRegistry::new(),
    );
    while let Some(event) = rx_a.recv().await {
        match &event {
            RuntimeEvent::HookPanicked { hook_name, message } => {
                println!("[event] HookPanicked hook={hook_name} msg={message}");
            }
            RuntimeEvent::RunCompleted { output } => println!("[event] RunCompleted: {output}"),
            _ => {}
        }
    }
    handle_a.wait().await;

    // Scenario B: hook returns Abort (run emits RunFailed)
    println!("\n=== Scenario B: hook aborts run ===");
    let config_b = AgentConfig::builder("mock/mock")
        .system_prompt("assistant")
        .max_steps(5)
        .build()
        .unwrap()
        .with_hook(Arc::new(AbortAfterNHook::new(0)));

    let (handle_b, mut rx_b) = AgentRun::start(
        config_b,
        "go".into(),
        Arc::new(MockModel),
        ToolRegistry::new(),
    );
    while let Some(event) = rx_b.recv().await {
        if let RuntimeEvent::RunFailed { error } = &event {
            println!("[event] RunFailed: {error}");
        }
    }
    handle_b.wait().await;
}
