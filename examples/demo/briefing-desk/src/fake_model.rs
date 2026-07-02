//! Deterministic fake model for the `--fake` smoke path. No network calls.
//!
//! This crate has no reusable fake `ModelAdapter` to import from Orchest itself
//! (see the v0.10 validation rubric: absence of a public fake is a recorded
//! modality-gateway/API-friction finding, not a workaround) so this mirrors the
//! pattern used by `examples/rust/resilience/session_persist_resume.rs`.
//!
//! The fake drives a real, deterministic tool-call sequence — search, then
//! read, then write — by inspecting the running message history for prior
//! `ToolResult` blocks rather than keeping internal mutable state. This
//! exercises the actual search/read/write tool contract (issue 003), not
//! just a canned final answer.

use async_trait::async_trait;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::tool::ToolDef;
use serde_json::{json, Value};
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
        messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let results = tool_results(messages);
        let content = plan(&results);
        let stop_reason = if matches!(content.first(), Some(ContentBlock::ToolUse { .. })) {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        };

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
            content,
            usage,
            stop_reason,
            option_adjustments: vec![],
        })
    }
}

/// Every `ToolResult` content payload seen so far, in message order.
fn tool_results(messages: &[Message]) -> Vec<&Value> {
    messages
        .iter()
        .flat_map(|m| &m.content)
        .filter_map(|block| match block {
            ContentBlock::ToolResult { content, .. } => Some(content),
            _ => None,
        })
        .collect()
}

/// Deterministic three-step plan: search -> read the top hit -> write the
/// report. Terminates with a text response once the write result (approved
/// or denied) comes back.
fn plan(results: &[&Value]) -> Vec<ContentBlock> {
    match results.len() {
        0 => vec![ContentBlock::ToolUse {
            id: "call-search".into(),
            name: "search_fixtures".into(),
            input: json!({"query": "retention", "top_k": 3}),
        }],
        1 => match top_search_hit_path(results[0]) {
            Some(path) => vec![ContentBlock::ToolUse {
                id: "call-read".into(),
                name: "read_fixture".into(),
                input: json!({"path": path}),
            }],
            None => vec![ContentBlock::Text(
                "search_fixtures returned no results; nothing to read.".into(),
            )],
        },
        2 => vec![ContentBlock::ToolUse {
            id: "call-write".into(),
            name: "write_report".into(),
            input: json!({"content": FAKE_BRIEF}),
        }],
        _ => vec![ContentBlock::Text(final_message(results.last().copied()))],
    }
}

fn top_search_hit_path(search_result: &Value) -> Option<String> {
    search_result
        .as_array()?
        .first()?
        .get("path")?
        .as_str()
        .map(str::to_string)
}

fn final_message(write_result: Option<&Value>) -> String {
    let denied = write_result
        .and_then(|v| v.get("error"))
        .and_then(|e| e.get("code"))
        .and_then(Value::as_str)
        == Some("APPROVAL_DENIED");
    if denied {
        "The report was not written because approval was denied.".to_string()
    } else {
        "Report written successfully.".to_string()
    }
}

/// Deterministic canned brief content passed to `write_report`. Report-shape
/// compliance (issue 006) and real materials-aware reasoning (issue 005 for
/// multimedia) are out of scope here — this only needs to exercise the
/// search -> read -> write -> approval pipeline offline.
const FAKE_BRIEF: &str = "\
# Briefing Desk (fake smoke run)

This is a deterministic placeholder brief produced by `--fake` mode. It exists to \
exercise the search/read/write tool pipeline end to end without network \
credentials; it is not a real answer to the question.
";
