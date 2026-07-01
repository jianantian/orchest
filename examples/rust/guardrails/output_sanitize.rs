//! OutputGuardrail example: replace sensitive tokens in model output.
//!
//! Run with: cargo run --example guardrail_output_sanitize

use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::guardrail::{OutputGuardrail, OutputGuardrailAction};
use orchest::hook::ModelHookContext;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::{registry::ToolRegistry, ToolDef};
use async_trait::async_trait;
use tokio::sync::mpsc;

// ── Mock model ────────────────────────────────────────────────────────────────

struct SensitiveModel;

#[async_trait]
impl ModelAdapter for SensitiveModel {
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
            output_tokens: 20,
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
            content: vec![ContentBlock::Text(
                "The user's SSN is 123-45-6789 and credit card 4111-1111-1111-1111.".into(),
            )],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Output sanitizer ──────────────────────────────────────────────────────────

struct SanitizeGuardrail;

#[async_trait]
impl OutputGuardrail for SanitizeGuardrail {
    async fn check(&self, ctx: &ModelHookContext) -> OutputGuardrailAction {
        if let Some(ref content) = ctx.response {
            let mut sanitized = content.clone();
            for block in &mut sanitized {
                if let ContentBlock::Text(ref mut t) = block {
                    // Mask SSN pattern
                    *t = t.replace(
                        |_c: char| false, // placeholder: no regex available, use simple contains
                        "",
                    );
                    // Simple keyword replacement
                    if t.contains("SSN") || t.contains("ssn") || t.contains("credit card") {
                        *t = "[REDACTED: sensitive content removed]".into();
                        println!("[guardrail] output sanitized: sensitive content detected");
                    }
                }
            }
            return OutputGuardrailAction::Replace(sanitized);
        }
        OutputGuardrailAction::Allow
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("You are a helpful assistant.")
        .max_steps(3)
        .build()
        .unwrap()
        .with_output_guardrail(Arc::new(SanitizeGuardrail));

    let (handle, mut rx) = AgentRun::start(
        config,
        "Tell me about the user.".into(),
        Arc::new(SensitiveModel),
        ToolRegistry::new(),
    );

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::RunCompleted { output } => {
                println!("[run] completed output: {output}");
                assert!(
                    !output.to_string().contains("123-45-6789"),
                    "SSN should be redacted"
                );
            }
            RuntimeEvent::RunFailed { error } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;
    println!("Done. Sensitive content was sanitized.");
}
