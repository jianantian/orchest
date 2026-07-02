//! Deterministic fake models for the `--fake` path. No network calls.
//!
//! Orchest has no reusable fake `ModelAdapter` to import (see the v0.10
//! validation rubric: absence of a public fake is a recorded API-friction
//! finding, not a workaround) so this mirrors the pattern used by
//! `examples/rust/resilience/session_persist_resume.rs`.
//!
//! [`FakeModel`] drives two deterministic behaviors by inspecting the running
//! message history, with no internal mutable state:
//! - **Fresh run**: search -> read -> review -> write, then finalize.
//! - **Resumed run with a follow-up question appended**: answer directly,
//!   quoting a snippet of the original brief pulled out of history, proving
//!   the resumed context actually carries the prior brief forward.
//!
//! [`ReviewerFakeModel`] backs the `review_report` Agent-as-Tool child: it
//! always returns one canned verdict, ignoring its input (`ContextMode::Fresh`
//! means it has no parent history to inspect anyway).

use async_trait::async_trait;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
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
        let content = plan(messages);
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

/// Always answers immediately with a canned review verdict. Used only as the
/// `review_report` sub-agent's model, which runs with `ContextMode::Fresh`
/// (no parent history), so there is nothing to inspect.
pub struct ReviewerFakeModel;

#[async_trait]
impl ModelAdapter for ReviewerFakeModel {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "fake-reviewer-model"
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
        Ok(ModelResponse {
            content: vec![ContentBlock::Text(REVIEW_VERDICT.to_string())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

const REVIEW_VERDICT: &str = "Reviewed against the fixture corpus: the retention conflict \
(42% finance-reconciled vs. 35% raw funnel) and the missing Tempo pricing point are both \
flagged rather than silently resolved; the interview quote is sourced via ASR, not the \
follow-up-notes paraphrase. Approved.";

fn plan(messages: &[Message]) -> Vec<ContentBlock> {
    if let Some(question) = pending_follow_up_question(messages) {
        return vec![ContentBlock::Text(follow_up_answer(&question, messages))];
    }

    match last_completed_tool(messages) {
        None => vec![search_call()],
        Some((name, result)) if name == "search_fixtures" => match top_search_hit_path(&result) {
            Some(path) => vec![read_call(&path)],
            None => vec![ContentBlock::Text(
                "search_fixtures returned no results; nothing to read.".into(),
            )],
        },
        Some((name, _)) if name == "read_fixture" => vec![review_call()],
        Some((name, review_result)) if name == "review_report" => {
            vec![write_call(&review_result)]
        }
        Some((name, write_result)) if name == "write_report" => {
            vec![ContentBlock::Text(final_message(Some(&write_result)))]
        }
        Some(_) => vec![ContentBlock::Text("unexpected tool call sequence.".into())],
    }
}

/// True when the *most recent* message is a fresh user question appended
/// after some tool activity already happened earlier in the session — i.e.
/// this is a resumed run's follow-up, not the very first message of a fresh
/// run (which also looks like a lone user Text message, but has no prior
/// `ToolResult`s).
fn pending_follow_up_question(messages: &[Message]) -> Option<String> {
    let last = messages.last()?;
    if last.role != Role::User {
        return None;
    }
    let text = last.content.iter().find_map(|b| match b {
        ContentBlock::Text(t) => Some(t.clone()),
        _ => None,
    })?;
    let has_prior_tool_result = messages
        .iter()
        .flat_map(|m| &m.content)
        .any(|b| matches!(b, ContentBlock::ToolResult { .. }));
    has_prior_tool_result.then_some(text)
}

fn follow_up_answer(question: &str, messages: &[Message]) -> String {
    match original_brief_snippet(messages) {
        Some(snippet) => format!(
            "Follow-up on \"{question}\": referencing the original brief (\"{snippet}\"), \
             no new information changes that conclusion."
        ),
        None => format!(
            "Follow-up on \"{question}\": no prior brief found in this session to reference."
        ),
    }
}

fn original_brief_snippet(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .flat_map(|m| &m.content)
        .find_map(|b| match b {
            ContentBlock::ToolUse { name, input, .. } if name == "write_report" => {
                input.get("content").and_then(Value::as_str).map(|s| {
                    s.lines()
                        .next()
                        .unwrap_or(s)
                        .chars()
                        .take(80)
                        .collect::<String>()
                })
            }
            _ => None,
        })
}

/// The tool name and result content of the most recently *completed* tool
/// call, found by matching the last `ToolResult` back to its originating
/// `ToolUse` by id.
fn last_completed_tool(messages: &[Message]) -> Option<(String, Value)> {
    let blocks: Vec<&ContentBlock> = messages.iter().flat_map(|m| &m.content).collect();
    let (tool_use_id, content) = blocks.iter().rev().find_map(|b| match b {
        ContentBlock::ToolResult {
            tool_use_id,
            content,
        } => Some((tool_use_id.clone(), content.clone())),
        _ => None,
    })?;
    let name = blocks.iter().find_map(|b| match b {
        ContentBlock::ToolUse { id, name, .. } if *id == tool_use_id => Some(name.clone()),
        _ => None,
    })?;
    Some((name, content))
}

fn search_call() -> ContentBlock {
    ContentBlock::ToolUse {
        id: "call-search".into(),
        name: "search_fixtures".into(),
        input: json!({"query": "retention", "top_k": 3}),
    }
}

fn read_call(path: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: "call-read".into(),
        name: "read_fixture".into(),
        input: json!({"path": path}),
    }
}

fn review_call() -> ContentBlock {
    ContentBlock::ToolUse {
        id: "call-review".into(),
        name: "review_report".into(),
        input: json!({"draft": FAKE_BRIEF}),
    }
}

fn write_call(review_result: &Value) -> ContentBlock {
    let verdict = review_result
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or("(no verdict)");
    let content = format!("{FAKE_BRIEF}\n---\nReviewer note: {verdict}\n");
    ContentBlock::ToolUse {
        id: "call-write".into(),
        name: "write_report".into(),
        input: json!({"content": content}),
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
/// search -> read -> review -> write -> approval pipeline offline.
const FAKE_BRIEF: &str = "\
# Briefing Desk (fake smoke run)

This is a deterministic placeholder brief produced by `--fake` mode. It exists to \
exercise the search/read/review/write tool pipeline end to end without network \
credentials; it is not a real answer to the question.
";
