//! Session persistence + resume example: a run records its session log in an
//! InMemorySessionStore, then a follow-up turn resumes the log.
//!
//! Run with: cargo run --example session_persist_resume

use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::RunInput;
use orchest::run::{AgentConfig, AgentRun};
use orchest::session::{InMemorySessionStore, SessionLog, SessionStore};
use orchest::tool::{registry::ToolRegistry, ToolDef};
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct EndModel;

#[async_trait]
impl ModelAdapter for EndModel {
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
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
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
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("Task complete.".into())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(InMemorySessionStore::new());
    const SESSION_ID: &str = "my-session";

    // ── Phase 1: First run ────────────────────────────────────────────────────

    println!("=== Phase 1: Starting first run ===");
    let config = AgentConfig::builder("persistent-agent", "mock/mock")
        .system_prompt("You are a helpful assistant.")
        .max_steps(3)
        .session_store(store.clone() as Arc<dyn SessionStore>, SESSION_ID)
        .build()?;

    let (handle, mut rx) = AgentRun::start(
        config,
        "Hello, what can you do?".into(),
        Arc::new(EndModel),
        ToolRegistry::new(),
    );
    let first_run_id = handle.run_id;

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output, .. } => println!("[run1] completed: {output}"),
            RuntimeEvent::RunFailed { error, .. } => println!("[run1] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;

    // ── Phase 2: Read the session log ─────────────────────────────────────────

    println!("\n=== Phase 2: Reading the session log ===");
    let log = SessionLog::from_events(store.read(SESSION_ID, 1).await?)?;
    let projection = log.projection();

    println!("[log] events: {}", log.events().len());
    println!("[log] run_id: {:?}", projection.run_id);
    println!("[log] next step: {}", projection.step);
    println!("[log] messages: {}", projection.messages.len());
    println!("[log] tokens used: {}", projection.budget_used.tokens_used);

    assert_eq!(
        projection.run_id,
        Some(first_run_id),
        "log run_id should match"
    );
    assert!(
        !projection.messages.is_empty(),
        "log should project messages"
    );

    // ── Phase 3: Resume with a follow-up turn ─────────────────────────────────

    println!("\n=== Phase 3: Resuming with a follow-up ===");
    // The host supplies the config again; only the log is restored.
    let config = AgentConfig::builder("persistent-agent", "mock/mock")
        .system_prompt("You are a helpful assistant.")
        .max_steps(3)
        .session_store(store.clone() as Arc<dyn SessionStore>, SESSION_ID)
        .build()?;

    let (handle2, mut rx2) = AgentRun::resume_with_input(
        config,
        RunInput::text("And what else?"),
        Arc::new(EndModel),
        ToolRegistry::new(),
    )
    .await?;

    println!(
        "[resume] run_id: {} (same as original: {})",
        handle2.run_id, first_run_id
    );
    assert_eq!(
        handle2.run_id, first_run_id,
        "resumed run_id must match original"
    );

    while let Some(event) = rx2.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output, .. } => println!("[run2] completed: {output}"),
            RuntimeEvent::RunFailed { error, .. } => println!("[run2] failed: {error}"),
            _ => {}
        }
    }
    handle2.wait().await;

    let log2 = SessionLog::from_events(store.read(SESSION_ID, 1).await?)?;
    println!(
        "\n[final log] events: {}, tokens used: {}",
        log2.events().len(),
        log2.projection().budget_used.tokens_used
    );

    println!("\nDone. Session persisted and resumed successfully.");
    Ok(())
}
