//! Deterministic fake model for the `--fake` smoke path. No network calls.
//!
//! This crate has no reusable fake `ModelAdapter` to import from Orchest itself
//! (see the v0.10 validation rubric: absence of a public fake is a recorded
//! modality-gateway/API-friction finding, not a workaround) so this mirrors the
//! pattern used by `examples/rust/resilience/session_persist_resume.rs`.

use async_trait::async_trait;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::tool::ToolDef;
use tokio::sync::mpsc;

pub struct FakeModel;

#[async_trait]
impl ModelAdapter for FakeModel {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "fake-briefing-model"
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
            input_tokens: 42,
            output_tokens: 17,
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
            content: vec![ContentBlock::Text(FAKE_BRIEF.to_string())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

/// Deterministic canned brief. Report-shape compliance (issue 006) and real
/// materials-aware reasoning (issues 003/005) are out of scope here — this
/// only needs to exercise the pipeline offline.
const FAKE_BRIEF: &str = "\
# Briefing Desk (fake smoke run)

This is a deterministic placeholder brief produced by `--fake` mode. It exists to \
exercise the run pipeline end to end without network credentials; it is not a real \
answer to the question and does not reason over the materials directory.
";
