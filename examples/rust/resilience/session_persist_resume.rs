//! Session persistence + resume example: save a snapshot with InMemorySessionStore,
//! then resume the run and continue from the saved state.
//!
//! Run with: cargo run --example session_persist_resume

use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::session::{InMemorySessionStore, SessionStore};
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
    let config = AgentConfig::builder("mock/mock")
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
            RuntimeEvent::RunCompleted { output } => println!("[run1] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run1] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;

    // ── Phase 2: Load snapshot ────────────────────────────────────────────────

    println!("\n=== Phase 2: Loading snapshot ===");
    let snapshot = store
        .load(SESSION_ID)
        .await?
        .expect("snapshot should exist after run");

    println!("[snapshot] run_id: {}", snapshot.run_id);
    println!("[snapshot] step: {}", snapshot.step);
    println!("[snapshot] messages: {}", snapshot.messages.len());
    println!(
        "[snapshot] tokens used: {}",
        snapshot.budget_used.tokens_used
    );

    assert_eq!(
        snapshot.run_id, first_run_id,
        "snapshot run_id should match"
    );
    assert!(
        !snapshot.messages.is_empty(),
        "snapshot should have messages"
    );

    // ── Phase 3: Resume ───────────────────────────────────────────────────────

    println!("\n=== Phase 3: Resuming run ===");
    // Re-attach session store so persistence continues on resume
    let mut snap = snapshot;
    snap.active_config = snap
        .active_config
        .with_session_store(store.clone() as Arc<dyn SessionStore>, SESSION_ID);

    let (handle2, mut rx2) = AgentRun::resume(snap, Arc::new(EndModel), ToolRegistry::new());

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
            RuntimeEvent::RunCompleted { output } => println!("[run2] completed: {output}"),
            RuntimeEvent::RunFailed { error } => println!("[run2] failed: {error}"),
            _ => {}
        }
    }
    handle2.wait().await;

    // Verify snapshot was updated by the resumed run
    let snap2 = store.load(SESSION_ID).await?.unwrap();
    println!(
        "\n[final snapshot] tokens used: {}",
        snap2.budget_used.tokens_used
    );

    println!("\nDone. Session persisted and resumed successfully.");
    Ok(())
}
