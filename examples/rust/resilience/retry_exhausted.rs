//! Example: RetryPolicy — 429 rate-limit retries followed by exhaustion.
//!
//! A model that always returns HTTP 429 is wired to a RetryPolicy with
//! max_retries = 2 and a short fixed backoff.  The run emits two ModelRetry
//! events then RunFailed once the budget is exhausted.
//!
//! Run with: cargo run --example retry_exhausted

use std::sync::Arc;
use std::time::Duration;

use orchest::events::RuntimeEvent;
use orchest::model::{
    Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse, RequestOptions,
    StreamEvent,
};
use orchest::run::{AgentConfig, AgentRun, BackoffStrategy, RetryPolicy};
use orchest::tool::registry::ToolRegistry;
use async_trait::async_trait;
use tokio::sync::mpsc;

// ── Always-failing model ──────────────────────────────────────────────────────

struct RateLimitedModel;

#[async_trait]
impl ModelAdapter for RateLimitedModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "rate-limited"
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
        Err(ModelError {
            message: "rate limit exceeded".into(),
            code: Some("rate_limit_exceeded".into()),
            provider: Some("mock".into()),
            status: Some(429),
            retry_after_secs: None,
            upstream: None,
        })
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AgentConfig::builder("mock/rate-limited")
        .system_prompt("assistant")
        .max_steps(5)
        .retry_policy(RetryPolicy {
            max_retries: 2,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(10)),
        })
        .build()
        .unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "do something".into(),
        Arc::new(RateLimitedModel),
        ToolRegistry::new(),
    );

    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::ModelRetry {
                attempt,
                error,
                next_delay,
            } => {
                println!("[event] ModelRetry attempt={attempt} error={error} delay={next_delay:?}");
            }
            RuntimeEvent::RunFailed { error } => {
                println!("[event] RunFailed: {error}");
            }
            _ => {}
        }
    }
    handle.wait().await;
}
