//! Deterministic fake models for the `--fake` path. No network calls.
//!
//! Orchest has no reusable fake `ModelAdapter` to import (see the v0.10
//! validation rubric: absence of a public fake is a recorded API-friction
//! finding, not a workaround) so this mirrors the pattern used by
//! `examples/rust/resilience/session_persist_resume.rs`.
//!
//! [`FakeModel`] drives two deterministic behaviors by inspecting the running
//! message history and the tools actually registered, with no internal
//! mutable state:
//! - **Fresh run**: walks a fixed pipeline — search, read, transcribe_audio
//!   and describe_image (only if those tools are registered, i.e. the corpus
//!   actually has audio/image sources), review, write, synthesize_brief
//!   (only if registered, i.e. not `--no-tts`) — skipping steps whose tool
//!   isn't registered and stopping the synthesize step if the write was
//!   denied. Each step's input is built from the *real* results of the steps
//!   before it (e.g. `read_fixture`'s path comes from parsing
//!   `search_fixtures`'s actual output).
//! - **Resumed run with a follow-up question appended**: answer directly,
//!   quoting a snippet of the original brief pulled out of history, proving
//!   the resumed context actually carries the prior brief forward.
//!
//! [`ReviewerFakeModel`] backs the `review_report` Agent-as-Tool child: it
//! always returns one canned verdict, ignoring its input (`ContextMode::Fresh`
//! means it has no parent history to inspect anyway).

use std::collections::HashSet;

use async_trait::async_trait;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
};
use orchest::tool::ToolDef;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Fixed step order. A step only runs if a tool of that name is registered
/// (search/read/review/write always are; transcribe_audio/describe_image are
/// corpus-dependent; synthesize_brief is absent under `--no-tts`).
const PIPELINE: &[&str] = &[
    "search_fixtures",
    "read_fixture",
    "transcribe_audio",
    "describe_image",
    "review_report",
    "write_report",
    "synthesize_brief",
];

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
        tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let content = plan(messages, tools);
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

fn plan(messages: &[Message], tools: &[ToolDef]) -> Vec<ContentBlock> {
    if let Some(question) = pending_follow_up_question(messages) {
        return vec![ContentBlock::Text(follow_up_answer(&question, messages))];
    }

    let available: HashSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let completed: HashSet<&str> = completed_tool_names(messages);

    for step in PIPELINE {
        if !available.contains(step) {
            continue;
        }
        if *step == "synthesize_brief" && write_was_denied(messages) {
            continue;
        }
        if completed.contains(step) {
            continue;
        }
        return vec![build_call(step, messages, tools)];
    }

    vec![ContentBlock::Text(finalize_message(messages))]
}

fn build_call(step: &str, messages: &[Message], tools: &[ToolDef]) -> ContentBlock {
    match step {
        "search_fixtures" => search_call(),
        "read_fixture" => {
            match find_tool_result(messages, "search_fixtures").and_then(top_search_hit_path) {
                Some(path) => read_call(&path),
                None => ContentBlock::Text(
                    "search_fixtures returned no results; nothing to read.".into(),
                ),
            }
        }
        "transcribe_audio" => ContentBlock::ToolUse {
            id: "call-transcribe".into(),
            name: "transcribe_audio".into(),
            input: json!({"path": default_path_for(tools, "transcribe_audio").unwrap_or_default()}),
        },
        "describe_image" => ContentBlock::ToolUse {
            id: "call-describe".into(),
            name: "describe_image".into(),
            input: json!({"path": default_path_for(tools, "describe_image").unwrap_or_default()}),
        },
        "review_report" => review_call(&draft_brief(messages)),
        "write_report" => write_call(&write_content(messages)),
        "synthesize_brief" => ContentBlock::ToolUse {
            id: "call-synthesize".into(),
            name: "synthesize_brief".into(),
            input: json!({"text": write_content(messages)}),
        },
        other => ContentBlock::Text(format!("unknown pipeline step '{other}'")),
    }
}

/// Reads the real, discovered corpus path back out of a tool's own input
/// schema (`properties.path.default`, set by `media.rs` at construction
/// time from the actual `--materials` directory). The fake model has no
/// other way to learn where `--materials` pointed — it never sees CLI args,
/// only `ToolDef`s and message history — so this is how it stays correct
/// regardless of whether `--materials` was relative or absolute.
fn default_path_for(tools: &[ToolDef], tool_name: &str) -> Option<String> {
    tools
        .iter()
        .find(|t| t.name == tool_name)?
        .input_schema
        .get("properties")?
        .get("path")?
        .get("default")?
        .as_str()
        .map(str::to_string)
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

/// Every tool name that has at least one completed `ToolResult` in history.
fn completed_tool_names(messages: &[Message]) -> HashSet<&str> {
    let blocks: Vec<&ContentBlock> = messages.iter().flat_map(|m| &m.content).collect();
    let result_ids: HashSet<&str> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect();
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, name, .. } if result_ids.contains(id.as_str()) => {
                Some(name.as_str())
            }
            _ => None,
        })
        .collect()
}

/// The result content of a specific tool's (first) completed call, found by
/// matching its `ToolUse` id to a `ToolResult`.
fn find_tool_result<'a>(messages: &'a [Message], name: &str) -> Option<&'a Value> {
    let blocks: Vec<&ContentBlock> = messages.iter().flat_map(|m| &m.content).collect();
    let id = blocks.iter().find_map(|b| match b {
        ContentBlock::ToolUse { id, name: n, .. } if n == name => Some(id.clone()),
        _ => None,
    })?;
    blocks.iter().find_map(|b| match b {
        ContentBlock::ToolResult {
            tool_use_id,
            content,
        } if *tool_use_id == id => Some(content),
        _ => None,
    })
}

fn write_was_denied(messages: &[Message]) -> bool {
    find_tool_result(messages, "write_report")
        .and_then(|v| v.get("error"))
        .and_then(|e| e.get("code"))
        .and_then(Value::as_str)
        == Some("APPROVAL_DENIED")
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

fn review_call(draft: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: "call-review".into(),
        name: "review_report".into(),
        input: json!({"draft": draft}),
    }
}

fn write_call(content: &str) -> ContentBlock {
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

/// Builds the draft brief from the base placeholder plus whatever
/// transcribe_audio/describe_image results are available so far, so "the
/// transcript feeds the brief" and "image-derived facts appear in the
/// brief" are literally true and checkable, not just claimed.
fn draft_brief(messages: &[Message]) -> String {
    let mut sections = vec![FAKE_BRIEF.trim().to_string()];
    if let Some(transcript) = find_tool_result(messages, "transcribe_audio")
        .and_then(|v| v.get("transcript"))
        .and_then(Value::as_str)
    {
        sections.push(format!(
            "\n## Interview transcript (via ASR)\n\n> {transcript}\n"
        ));
    }
    if let Some(description) = find_tool_result(messages, "describe_image")
        .and_then(|v| v.get("description"))
        .and_then(Value::as_str)
    {
        sections.push(format!(
            "\n## Chart (via vision placeholder)\n\n{description}\n"
        ));
    }
    sections.join("\n")
}

fn write_content(messages: &[Message]) -> String {
    let verdict = find_tool_result(messages, "review_report")
        .and_then(|v| v.get("output"))
        .and_then(Value::as_str)
        .unwrap_or("(no verdict)");
    format!("{}\n---\nReviewer note: {verdict}\n", draft_brief(messages))
}

fn finalize_message(messages: &[Message]) -> String {
    if write_was_denied(messages) {
        "The report was not written because approval was denied.".to_string()
    } else {
        "Report written successfully.".to_string()
    }
}

/// Base placeholder brief content. Report-shape compliance (issue 006) is out
/// of scope here — this only needs to exercise the pipeline offline.
const FAKE_BRIEF: &str = "\
# Briefing Desk (fake smoke run)

This is a deterministic placeholder brief produced by `--fake` mode. It exists to \
exercise the search/read/review/write tool pipeline end to end without network \
credentials; it is not a real answer to the question.
";
