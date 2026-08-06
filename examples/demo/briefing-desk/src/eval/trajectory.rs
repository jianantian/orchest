//! Schema-versioned, sanitized trajectory events derived from `RuntimeEvent`.
//!
//! Trajectory lines are **not** a direct serde of runtime events. Conversion is
//! allowlist-based: stream chunks and nested thinking are dropped, secret-key
//! fields are masked, and `EventsDropped` marks the attempt inconclusive.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use orchest::budget::{BudgetConfig, BudgetUsage};
use orchest::events::{ApprovalContext, RuntimeEvent};
use orchest::model::{OptionAdjustment, StopReason, TokenUsage};
use orchest::run::RunId;
use orchest::tool::async_job::JobStatus;
use orchest::tool::{ToolCall, ToolError, ToolMetadata};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Trajectory event schema version accepted by this build.
pub const TRAJECTORY_SCHEMA_VERSION: &str = "1";

/// Replacement token written over secret-looking string values.
pub const SECRET_REDACTION: &str = "[REDACTED]";

/// Parent/child relationship for a recorded event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRelation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_depth: Option<u32>,
}

/// One sanitized trajectory line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryEvent {
    pub schema_version: String,
    pub sequence: u64,
    pub elapsed_ms: u64,
    pub run_relation: RunRelation,
    pub kind: String,
    pub data: Value,
}

/// Outcome of converting one `RuntimeEvent`.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizeOutcome {
    /// Zero or one retained event (stream chunks yield none).
    pub event: Option<TrajectoryEvent>,
    /// Set when this event requires the attempt to be marked inconclusive.
    pub mark_inconclusive: bool,
}

/// Collects ordered trajectory events for one attempt.
#[derive(Debug)]
pub struct TrajectoryRecorder {
    start: Instant,
    sequence: u64,
    events: Vec<TrajectoryEvent>,
    inconclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedTerminal {
    pub kind: String,
    pub stop_reason: Option<String>,
}

impl Default for TrajectoryRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl TrajectoryRecorder {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            sequence: 0,
            events: Vec::new(),
            inconclusive: false,
        }
    }

    /// Observe a runtime event: convert via allowlist and append if retained.
    pub fn observe(&mut self, event: &RuntimeEvent) {
        let elapsed_ms = self.start.elapsed().as_millis() as u64;
        let outcome =
            sanitize_runtime_event(event, self.sequence, elapsed_ms, RunRelation::default());
        if outcome.mark_inconclusive {
            self.inconclusive = true;
        }
        if let Some(ev) = outcome.event {
            self.sequence = self.sequence.saturating_add(1);
            self.events.push(ev);
        }
    }

    /// Record the application-level seed identity retained by a real resume.
    pub fn record_followup_session_resumed(&mut self, seed_id: &str, seed_hash: &str) {
        let event = TrajectoryEvent {
            schema_version: TRAJECTORY_SCHEMA_VERSION.to_string(),
            sequence: self.sequence,
            elapsed_ms: self.start.elapsed().as_millis() as u64,
            run_relation: RunRelation::default(),
            kind: "followup_session_resumed".into(),
            data: json!({
                "session_seed_id": seed_id,
                "session_seed_hash": seed_hash,
            }),
        };
        self.sequence = self.sequence.saturating_add(1);
        self.events.push(event);
    }

    pub fn events(&self) -> &[TrajectoryEvent] {
        &self.events
    }

    pub fn is_inconclusive(&self) -> bool {
        self.inconclusive
    }

    pub fn stream_started(&self) -> bool {
        self.events.iter().any(|event| event.kind == "run_started")
    }

    pub fn events_dropped(&self) -> bool {
        self.events
            .iter()
            .any(|event| event.kind == "events_dropped")
    }

    /// Last retained top-level terminal; nested child terminals are excluded.
    pub fn terminal(&self) -> Option<RetainedTerminal> {
        self.events.iter().rev().find_map(|event| {
            if event.run_relation.parent_run_id.is_some()
                || event.run_relation.child_run_id.is_some()
            {
                return None;
            }
            match event.kind.as_str() {
                "run_completed" => Some(RetainedTerminal {
                    kind: event.kind.clone(),
                    stop_reason: event
                        .data
                        .get("stop_reason")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                }),
                "run_failed" | "run_aborted" => Some(RetainedTerminal {
                    kind: event.kind.clone(),
                    stop_reason: None,
                }),
                _ => None,
            }
        })
    }

    #[allow(dead_code)]
    pub fn mark_inconclusive(&mut self) {
        self.inconclusive = true;
    }

    /// Write every event as one JSON object per line.
    pub fn write_jsonl(&self, path: &Path) -> Result<(), TrajectoryError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| TrajectoryError::io(format!("creating {}: {e}", parent.display())))?;
        }
        let file = File::create(path)
            .map_err(|e| TrajectoryError::io(format!("creating {}: {e}", path.display())))?;
        let mut writer = BufWriter::new(file);
        for event in &self.events {
            serde_json::to_writer(&mut writer, event).map_err(|e| {
                TrajectoryError::serialize(format!("writing trajectory event: {e}"))
            })?;
            writer
                .write_all(b"\n")
                .map_err(|e| TrajectoryError::io(format!("writing newline: {e}")))?;
        }
        writer
            .flush()
            .map_err(|e| TrajectoryError::io(format!("flushing trajectory: {e}")))?;
        Ok(())
    }
}

/// Errors from trajectory conversion or I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrajectoryError {
    pub message: String,
}

impl TrajectoryError {
    pub fn io(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn serialize(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TrajectoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TrajectoryError {}

/// Convert one runtime event into zero or one trajectory events.
#[allow(clippy::too_many_lines)]
pub fn sanitize_runtime_event(
    event: &RuntimeEvent,
    sequence: u64,
    elapsed_ms: u64,
    relation: RunRelation,
) -> SanitizeOutcome {
    match event {
        RuntimeEvent::ModelStreamChunk { .. } => SanitizeOutcome {
            event: None,
            mark_inconclusive: false,
        },

        RuntimeEvent::ChildRunEvent {
            child_run_id,
            run_depth,
            event: nested,
        } => {
            let nested_relation = RunRelation {
                parent_run_id: relation
                    .child_run_id
                    .clone()
                    .or(relation.parent_run_id.clone()),
                child_run_id: Some(child_run_id.to_string()),
                run_depth: Some(*run_depth),
            };
            // Preserve outer parent if present for nested wrappers.
            let nested_relation =
                if relation.parent_run_id.is_some() && nested_relation.parent_run_id.is_none() {
                    RunRelation {
                        parent_run_id: relation.parent_run_id.clone(),
                        child_run_id: Some(child_run_id.to_string()),
                        run_depth: Some(*run_depth),
                    }
                } else {
                    nested_relation
                };
            sanitize_runtime_event(nested, sequence, elapsed_ms, nested_relation)
        }

        RuntimeEvent::SubAgentEvent {
            parent_run_id,
            child_run_id,
            event: nested,
        } => {
            let nested_relation = RunRelation {
                parent_run_id: Some(parent_run_id.to_string()),
                child_run_id: Some(child_run_id.to_string()),
                run_depth: relation.run_depth,
            };
            sanitize_runtime_event(nested, sequence, elapsed_ms, nested_relation)
        }

        RuntimeEvent::EventsDropped {
            subscriber_id,
            count,
        } => SanitizeOutcome {
            event: Some(TrajectoryEvent {
                schema_version: TRAJECTORY_SCHEMA_VERSION.to_string(),
                sequence,
                elapsed_ms,
                run_relation: relation,
                kind: "events_dropped".into(),
                data: json!({
                    "subscriber_id": subscriber_id,
                    "count": count,
                }),
            }),
            mark_inconclusive: true,
        },

        other => {
            let (kind, data) = match other {
                RuntimeEvent::RunStarted { run_id } => {
                    ("run_started", json!({ "run_id": run_id.to_string() }))
                }
                RuntimeEvent::ModelCallStarted { step } => {
                    ("model_call_started", json!({ "step": step }))
                }
                RuntimeEvent::ModelCallCompleted {
                    tokens,
                    option_adjustments,
                } => (
                    "model_call_completed",
                    json!({
                        "tokens": sanitize_token_usage(tokens),
                        "option_adjustments": option_adjustments
                            .iter()
                            .map(sanitize_option_adjustment)
                            .collect::<Vec<_>>(),
                    }),
                ),
                RuntimeEvent::ModelRetry {
                    attempt,
                    error,
                    next_delay,
                } => (
                    "model_retry",
                    json!({
                        "attempt": attempt,
                        "error": sanitize_free_text(error),
                        "next_delay_ms": duration_ms(next_delay),
                    }),
                ),
                RuntimeEvent::ToolCallStarted {
                    tool,
                    metadata,
                    input,
                } => (
                    "tool_call_started",
                    json!({
                        "tool": tool,
                        "metadata": sanitize_tool_metadata(metadata),
                        "input": sanitize_value(input),
                    }),
                ),
                RuntimeEvent::ToolCallUpdate {
                    tool,
                    tool_call_id,
                    partial,
                } => (
                    "tool_call_update",
                    json!({
                        "tool": tool,
                        "tool_call_id": tool_call_id,
                        "partial": sanitize_value(partial),
                    }),
                ),
                RuntimeEvent::ToolCallCompleted {
                    tool,
                    output,
                    duration,
                } => (
                    "tool_call_completed",
                    json!({
                        "tool": tool,
                        "output": sanitize_value(output),
                        "duration_ms": duration_ms(duration),
                    }),
                ),
                RuntimeEvent::ToolCallFailed { tool, error } => (
                    "tool_call_failed",
                    json!({
                        "tool": tool,
                        "error": sanitize_tool_error(error),
                    }),
                ),
                RuntimeEvent::ToolCallRetry {
                    tool,
                    attempt,
                    previous_error,
                    next_delay,
                } => (
                    "tool_call_retry",
                    json!({
                        "tool": tool,
                        "attempt": attempt,
                        "previous_error": sanitize_tool_error(previous_error),
                        "next_delay_ms": duration_ms(next_delay),
                    }),
                ),
                RuntimeEvent::ToolCallBatchStarted {
                    batch_id,
                    tool_count,
                } => (
                    "tool_call_batch_started",
                    json!({
                        "batch_id": batch_id,
                        "tool_count": tool_count,
                    }),
                ),
                RuntimeEvent::ToolCallBatchItemStarted {
                    batch_id,
                    tool,
                    requested_order,
                } => (
                    "tool_call_batch_item_started",
                    json!({
                        "batch_id": batch_id,
                        "tool": tool,
                        "requested_order": requested_order,
                    }),
                ),
                RuntimeEvent::ToolCallBatchItemCompleted {
                    batch_id,
                    tool,
                    requested_order,
                    completion_order,
                } => (
                    "tool_call_batch_item_completed",
                    json!({
                        "batch_id": batch_id,
                        "tool": tool,
                        "requested_order": requested_order,
                        "completion_order": completion_order,
                    }),
                ),
                RuntimeEvent::AsyncToolStarted { tool, job_id } => (
                    "async_tool_started",
                    json!({ "tool": tool, "job_id": job_id }),
                ),
                RuntimeEvent::AsyncToolProgress {
                    tool,
                    job_id,
                    status,
                } => (
                    "async_tool_progress",
                    json!({
                        "tool": tool,
                        "job_id": job_id,
                        "status": sanitize_job_status(status),
                    }),
                ),
                RuntimeEvent::AsyncToolCompleted {
                    tool,
                    job_id,
                    output,
                    elapsed,
                } => (
                    "async_tool_completed",
                    json!({
                        "tool": tool,
                        "job_id": job_id,
                        "output": sanitize_value(output),
                        "elapsed_ms": duration_ms(elapsed),
                    }),
                ),
                RuntimeEvent::SkillContentRead {
                    skill_name,
                    file,
                    tokens,
                } => (
                    "skill_content_read",
                    json!({
                        "skill_name": skill_name,
                        "file": file,
                        "tokens": tokens,
                    }),
                ),
                RuntimeEvent::ApprovalRequested { tool_call, context } => (
                    "approval_requested",
                    json!({
                        "tool_call": sanitize_tool_call(tool_call),
                        "context": sanitize_approval_context(context),
                    }),
                ),
                RuntimeEvent::ApprovalGranted { tool_call, context } => (
                    "approval_granted",
                    json!({
                        "tool_call": sanitize_tool_call(tool_call),
                        "context": sanitize_approval_context(context),
                    }),
                ),
                RuntimeEvent::ApprovalDenied { tool_call, context } => (
                    "approval_denied",
                    json!({
                        "tool_call": sanitize_tool_call(tool_call),
                        "context": sanitize_approval_context(context),
                    }),
                ),
                RuntimeEvent::BudgetWarning { used, limit } => (
                    "budget_warning",
                    json!({
                        "used": sanitize_budget_usage(used),
                        "limit": sanitize_budget_config(limit),
                    }),
                ),
                RuntimeEvent::RuntimeWarning { message } => (
                    "runtime_warning",
                    json!({ "message": sanitize_free_text(message) }),
                ),
                RuntimeEvent::SkillMissingCapabilities { skill_name } => (
                    "skill_missing_capabilities",
                    json!({ "skill_name": skill_name }),
                ),
                RuntimeEvent::SkillLoadWarning { path, reason } => (
                    "skill_load_warning",
                    json!({
                        "path": path,
                        "reason": sanitize_free_text(reason),
                    }),
                ),
                RuntimeEvent::ContextCompacted {
                    removed_messages,
                    summary_tokens,
                } => (
                    "context_compacted",
                    json!({
                        "removed_messages": removed_messages,
                        "summary_tokens": summary_tokens,
                    }),
                ),
                RuntimeEvent::SubAgentStarted {
                    parent_run_id,
                    child_run_id,
                    config_summary: _,
                } => (
                    "sub_agent_started",
                    json!({
                        "parent_run_id": parent_run_id.to_string(),
                        "child_run_id": child_run_id.to_string(),
                        // config_summary intentionally omitted
                    }),
                ),
                RuntimeEvent::SubAgentCompleted {
                    child_run_id,
                    output,
                    budget_used,
                } => (
                    "sub_agent_completed",
                    json!({
                        "child_run_id": child_run_id.to_string(),
                        "output": sanitize_value(output),
                        "budget_used": sanitize_budget_usage(budget_used),
                    }),
                ),
                RuntimeEvent::SubAgentFailed {
                    child_run_id,
                    error,
                } => (
                    "sub_agent_failed",
                    json!({
                        "child_run_id": child_run_id.to_string(),
                        "error": sanitize_free_text(error),
                    }),
                ),
                RuntimeEvent::HookPanicked { hook_name, message } => (
                    "hook_panicked",
                    json!({
                        "hook_name": hook_name,
                        "message": sanitize_free_text(message),
                    }),
                ),
                RuntimeEvent::AgentUpdated {
                    previous_agent,
                    new_agent,
                } => (
                    "agent_updated",
                    json!({
                        "previous_agent": previous_agent,
                        "new_agent": new_agent,
                    }),
                ),
                RuntimeEvent::RunRestarted { attempt } => {
                    ("run_restarted", json!({ "attempt": attempt }))
                }
                RuntimeEvent::RunCompleted {
                    output,
                    stop_reason,
                } => (
                    "run_completed",
                    json!({
                        "output": sanitize_value(output),
                        "stop_reason": stop_reason_label(stop_reason),
                    }),
                ),
                RuntimeEvent::RunFailed { error, .. } => {
                    ("run_failed", json!({ "error": sanitize_free_text(error) }))
                }
                RuntimeEvent::RunAborted { reason } => (
                    "run_aborted",
                    json!({
                        "reason": reason.as_ref().map(|r| sanitize_free_text(r)),
                    }),
                ),
                // Exhaustive arms handled above (stream / nested / dropped).
                RuntimeEvent::ModelStreamChunk { .. }
                | RuntimeEvent::ChildRunEvent { .. }
                | RuntimeEvent::SubAgentEvent { .. }
                | RuntimeEvent::EventsDropped { .. } => unreachable!(),
            };

            SanitizeOutcome {
                event: Some(TrajectoryEvent {
                    schema_version: TRAJECTORY_SCHEMA_VERSION.to_string(),
                    sequence,
                    elapsed_ms,
                    run_relation: relation,
                    kind: kind.into(),
                    data,
                }),
                mark_inconclusive: false,
            }
        }
    }
}

fn duration_ms(d: &Duration) -> u64 {
    d.as_millis() as u64
}

fn stop_reason_label(reason: &StopReason) -> String {
    match reason {
        StopReason::EndTurn => "end_turn".into(),
        StopReason::ToolUse => "tool_use".into(),
        StopReason::MaxTokens => "max_tokens".into(),
        StopReason::StopSequence => "stop_sequence".into(),
        StopReason::ContentFilter => "content_filter".into(),
        StopReason::Refusal => "refusal".into(),
        StopReason::ContextWindowExceeded => "context_window_exceeded".into(),
        StopReason::Pause => "pause".into(),
        StopReason::Interrupted => "interrupted".into(),
        StopReason::Other(s) => format!("other:{s}"),
    }
}

fn sanitize_token_usage(tokens: &TokenUsage) -> Value {
    // TokenUsage is already non-secret numeric fields; re-encode via serde then
    // strip any nested detail maps that might carry free-form keys.
    let mut v = serde_json::to_value(tokens).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        if let Some(details) = obj.get_mut("details") {
            *details = sanitize_value(details);
        }
    }
    v
}

fn sanitize_option_adjustment(adj: &OptionAdjustment) -> Value {
    json!({
        "option": adj.option,
        "requested": sanitize_value(&adj.requested),
        "applied": sanitize_value(&adj.applied),
        "reason": sanitize_free_text(&adj.reason),
    })
}

fn sanitize_tool_metadata(meta: &ToolMetadata) -> Value {
    // Keep grader-relevant metadata only; descriptions live in harness.
    let mut v = serde_json::to_value(meta).unwrap_or(Value::Null);
    v = sanitize_value(&v);
    v
}

fn sanitize_tool_error(error: &ToolError) -> Value {
    json!({
        "message": sanitize_free_text(&error.message),
        "kind": format!("{:?}", error.kind),
        "retry": format!("{:?}", error.retry),
        "code": error.code,
        "next_step": error.next_step.as_ref().map(|s| sanitize_free_text(s)),
    })
}

fn sanitize_tool_call(call: &ToolCall) -> Value {
    json!({
        "id": call.id,
        "name": call.name,
        "input": sanitize_value(&call.input),
    })
}

fn sanitize_approval_context(ctx: &ApprovalContext) -> Value {
    match ctx {
        ApprovalContext::InitialToolCall => json!({ "kind": "initial_tool_call" }),
        ApprovalContext::CommitToolCall { draft_tool } => {
            json!({ "kind": "commit_tool_call", "draft_tool": draft_tool })
        }
        ApprovalContext::RetryAfterFailure { .. } => {
            // Keep the discriminant without free-form failure payload leakage.
            json!({ "kind": "retry_after_failure" })
        }
    }
}

fn sanitize_budget_usage(used: &BudgetUsage) -> Value {
    json!({
        "tokens_used": used.tokens_used,
        "tool_calls_used": used.tool_calls_used,
        "cost_usd": used.cost_usd,
    })
}

fn sanitize_budget_config(limit: &BudgetConfig) -> Value {
    json!({
        "max_tokens": limit.max_tokens,
        "max_tool_calls": limit.max_tool_calls,
        "max_duration_ms": limit.max_duration.map(|d| duration_ms(&d)),
        "max_cost_usd": limit.max_cost_usd,
    })
}

fn sanitize_job_status(status: &JobStatus) -> Value {
    match status {
        JobStatus::Pending { progress, message } => json!({
            "kind": "pending",
            "progress": progress,
            "message": message.as_ref().map(|m| sanitize_free_text(m)),
        }),
        JobStatus::Completed(v) => json!({
            "kind": "completed",
            "output": sanitize_value(v),
        }),
        JobStatus::Failed(msg) => json!({
            "kind": "failed",
            "error": sanitize_free_text(msg),
        }),
    }
}

/// Recursively strip thinking/reasoning fields and mask secret-key values.
pub fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                if is_dropped_payload_key(key) {
                    continue;
                }
                if is_secret_key(key) {
                    out.insert(key.clone(), Value::String(SECRET_REDACTION.into()));
                } else {
                    out.insert(key.clone(), sanitize_value(child));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sanitize_value).collect()),
        Value::String(s) => Value::String(sanitize_free_text(s)),
        other => other.clone(),
    }
}

/// Keys whose values are dropped entirely (thinking / provider detail channels).
fn is_dropped_payload_key(key: &str) -> bool {
    matches!(
        normalize_key(key).as_str(),
        "thinking"
            | "reasoning"
            | "reasoningcontent"
            | "reasoningdetails"
            | "signature"
            | "providerdetails"
            | "hiddenthinking"
    )
}

/// Keys whose string values are replaced with `[REDACTED]`.
pub fn is_secret_key(key: &str) -> bool {
    super::credential::is_sensitive_configuration_name(key)
}

/// Lowercase and strip `_`, `-`, and whitespace for key matching.
pub fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|c| *c != '_' && *c != '-' && !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Best-effort free-text redaction for error strings (does not read env).
pub fn sanitize_free_text(text: &str) -> String {
    super::credential::redact_credentials_in_text(text, SECRET_REDACTION)
}

/// Helper for tests constructing nested child wrappers.
#[cfg(test)]
#[allow(dead_code)]
pub fn wrap_child(child_run_id: RunId, run_depth: u32, event: RuntimeEvent) -> RuntimeEvent {
    RuntimeEvent::ChildRunEvent {
        child_run_id,
        run_depth,
        event: Box::new(event),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::credential::text_has_credentials;
    use orchest::model::StreamEvent;
    use orchest::tool::{Approval, ToolExecutionMode, ToolParallelism, ToolSource};
    use serde_json::json;
    use std::time::Duration;

    fn run_id() -> RunId {
        RunId::new()
    }

    fn take_kind(event: &RuntimeEvent) -> Option<String> {
        let outcome = sanitize_runtime_event(event, 0, 0, RunRelation::default());
        outcome.event.map(|e| e.kind)
    }

    #[test]
    fn drops_all_model_stream_chunks_including_thinking() {
        let chunks = [
            StreamEvent::Text {
                delta: "hello".into(),
            },
            StreamEvent::ThinkingStart,
            StreamEvent::Thinking {
                delta: "secret thoughts".into(),
            },
            StreamEvent::ThinkingEnd {
                signature: Some("sig".into()),
                provider_details: Some(json!({"raw": true})),
            },
            StreamEvent::ToolUseStart {
                id: "t1".into(),
                name: "search".into(),
            },
            StreamEvent::Done {
                usage: TokenUsage::default(),
            },
        ];
        for chunk in chunks {
            let event = RuntimeEvent::ModelStreamChunk { delta: chunk };
            let outcome = sanitize_runtime_event(&event, 0, 0, RunRelation::default());
            assert!(outcome.event.is_none(), "chunk must be dropped");
            assert!(!outcome.mark_inconclusive);
        }
    }

    #[test]
    fn drops_nested_thinking_in_child_and_subagent_wrappers() {
        let thinking = RuntimeEvent::ModelStreamChunk {
            delta: StreamEvent::Thinking {
                delta: "nested".into(),
            },
        };
        let child = RuntimeEvent::ChildRunEvent {
            child_run_id: run_id(),
            run_depth: 1,
            event: Box::new(thinking.clone()),
        };
        let sub = RuntimeEvent::SubAgentEvent {
            parent_run_id: run_id(),
            child_run_id: run_id(),
            event: Box::new(thinking),
        };
        assert!(sanitize_runtime_event(&child, 0, 0, RunRelation::default())
            .event
            .is_none());
        assert!(sanitize_runtime_event(&sub, 0, 0, RunRelation::default())
            .event
            .is_none());
    }

    #[test]
    fn events_dropped_marks_inconclusive_and_retains_count() {
        let event = RuntimeEvent::EventsDropped {
            subscriber_id: 7,
            count: 3,
        };
        let outcome = sanitize_runtime_event(&event, 5, 12, RunRelation::default());
        assert!(outcome.mark_inconclusive);
        let te = outcome.event.expect("retained");
        assert_eq!(te.kind, "events_dropped");
        assert_eq!(te.sequence, 5);
        assert_eq!(te.data["count"], 3);
        assert_eq!(te.data["subscriber_id"], 7);
    }

    #[test]
    fn sub_agent_started_drops_config_summary() {
        let event = RuntimeEvent::SubAgentStarted {
            parent_run_id: run_id(),
            child_run_id: run_id(),
            config_summary: json!({
                "system_prompt": "do not record me",
                "api_key": "sk-secret",
            }),
        };
        let outcome = sanitize_runtime_event(&event, 0, 0, RunRelation::default());
        let te = outcome.event.expect("retained");
        assert_eq!(te.kind, "sub_agent_started");
        let s = serde_json::to_string(&te.data).unwrap();
        assert!(!s.contains("do not record me"));
        assert!(!s.contains("sk-secret"));
        assert!(!s.contains("config_summary"));
        assert!(te.data.get("parent_run_id").is_some());
        assert!(te.data.get("child_run_id").is_some());
    }

    #[test]
    fn recursive_value_sanitizer_drops_thinking_and_masks_secrets() {
        let value = json!({
            "tool": "search",
            "thinking": "hidden chain of thought",
            "reasoning": "also hidden",
            "signature": "sig-1",
            "provider_details": {"x": 1},
            "api_key": "sk-live-secret",
            "Authorization": "Bearer abc",
            "x-api-key": "k",
            "access_token": "tok",
            "set-cookie": "session=1",
            "cookie": "a=b",
            "nested": {
                "API-Key": "nested-secret",
                "output": "ok",
                "child": {
                    "thinking": "deep",
                    "result": 1
                }
            },
            "items": [
                {"reasoning": "nope", "n": 1},
                "plain"
            ]
        });
        let cleaned = sanitize_value(&value);
        let s = serde_json::to_string(&cleaned).unwrap();
        assert!(!s.contains("hidden chain"));
        assert!(!s.contains("also hidden"));
        assert!(!s.contains("sig-1"));
        assert!(!s.contains("sk-live-secret"));
        assert!(!s.contains("Bearer abc"));
        assert!(!s.contains("nested-secret"));
        assert!(!s.contains("\"thinking\""));
        assert!(!s.contains("\"reasoning\""));
        assert!(!s.contains("provider_details"));
        assert_eq!(cleaned["api_key"], SECRET_REDACTION);
        assert_eq!(cleaned["Authorization"], SECRET_REDACTION);
        assert_eq!(cleaned["nested"]["API-Key"], SECRET_REDACTION);
        assert_eq!(cleaned["nested"]["output"], "ok");
        assert_eq!(cleaned["nested"]["child"]["result"], 1);
        assert!(cleaned["nested"]["child"].get("thinking").is_none());
        assert_eq!(cleaned["items"][0]["n"], 1);
        assert!(cleaned["items"][0].get("reasoning").is_none());
    }

    #[test]
    fn retains_tool_name_order_approval_usage_and_terminal() {
        let meta = ToolMetadata {
            side_effect: true,
            approval: Approval::Always,
            execution_mode: ToolExecutionMode::Normal,
            parallelism: ToolParallelism::Serial,
            cost_hint: None,
            timeout: Some(Duration::from_secs(5)),
            max_output_tokens: Some(100),
            source: ToolSource::InProcess,
        };
        let started = RuntimeEvent::ToolCallStarted {
            tool: "write_report".into(),
            metadata: meta,
            input: json!({"path": "out.md", "api_key": "secret"}),
        };
        let completed = RuntimeEvent::ToolCallCompleted {
            tool: "write_report".into(),
            output: json!({"ok": true, "thinking": "drop me"}),
            duration: Duration::from_millis(42),
        };
        let approval = RuntimeEvent::ApprovalRequested {
            tool_call: ToolCall {
                id: "c1".into(),
                name: "write_report".into(),
                input: json!({"draft": "x", "authorization": "Bearer z"}),
            },
            context: ApprovalContext::InitialToolCall,
        };
        let usage = RuntimeEvent::ModelCallCompleted {
            tokens: TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                ..Default::default()
            },
            option_adjustments: vec![],
        };
        let terminal = RuntimeEvent::RunCompleted {
            output: json!("final brief"),
            stop_reason: StopReason::EndTurn,
        };

        let mut rec = TrajectoryRecorder::new();
        for e in [started, completed, approval, usage, terminal] {
            rec.observe(&e);
        }
        assert!(!rec.is_inconclusive());
        assert_eq!(rec.events().len(), 5);
        assert_eq!(rec.events()[0].sequence, 0);
        assert_eq!(rec.events()[1].sequence, 1);
        assert_eq!(rec.events()[0].kind, "tool_call_started");
        assert_eq!(rec.events()[0].data["tool"], "write_report");
        assert_eq!(rec.events()[0].data["input"]["api_key"], SECRET_REDACTION);
        assert_eq!(rec.events()[1].data["duration_ms"], 42);
        assert!(rec.events()[1].data["output"].get("thinking").is_none());
        assert_eq!(rec.events()[2].data["tool_call"]["name"], "write_report");
        assert_eq!(
            rec.events()[2].data["tool_call"]["input"]["authorization"],
            SECRET_REDACTION
        );
        assert_eq!(rec.events()[3].data["tokens"]["input_tokens"], 10);
        assert_eq!(rec.events()[4].kind, "run_completed");
        assert_eq!(rec.events()[4].data["stop_reason"], "end_turn");
        assert_eq!(rec.events()[4].data["output"], "final brief");
    }

    #[test]
    fn nested_child_preserves_relation_and_sanitizes_payload() {
        let parent = run_id();
        let child = run_id();
        let nested = RuntimeEvent::ToolCallCompleted {
            tool: "search_fixtures".into(),
            output: json!({"hits": 1, "api_key": "nope", "thinking": "x"}),
            duration: Duration::from_millis(1),
        };
        let event = RuntimeEvent::SubAgentEvent {
            parent_run_id: parent,
            child_run_id: child,
            event: Box::new(nested),
        };
        let outcome = sanitize_runtime_event(&event, 3, 9, RunRelation::default());
        let te = outcome.event.expect("retained");
        assert_eq!(te.kind, "tool_call_completed");
        let parent_s = parent.to_string();
        assert_eq!(
            te.run_relation.parent_run_id.as_deref(),
            Some(parent_s.as_str())
        );
        let child_s = child.to_string();
        assert_eq!(
            te.run_relation.child_run_id.as_deref(),
            Some(child_s.as_str())
        );
        assert_eq!(te.data["output"]["api_key"], SECRET_REDACTION);
        assert!(te.data["output"].get("thinking").is_none());
    }

    #[test]
    fn jsonl_writer_emits_independently_parseable_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trajectory.jsonl");
        let mut rec = TrajectoryRecorder::new();
        rec.observe(&RuntimeEvent::RunStarted { run_id: run_id() });
        rec.observe(&RuntimeEvent::RunCompleted {
            output: json!("done"),
            stop_reason: StopReason::EndTurn,
        });
        rec.write_jsonl(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = raw.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let v: TrajectoryEvent = serde_json::from_str(line).expect("parse line");
            assert_eq!(v.schema_version, TRAJECTORY_SCHEMA_VERSION);
        }
        // sequences strictly increasing
        assert!(rec.events()[0].sequence < rec.events()[1].sequence);
    }

    #[test]
    fn recorder_marks_inconclusive_on_events_dropped() {
        let mut rec = TrajectoryRecorder::new();
        rec.observe(&RuntimeEvent::EventsDropped {
            subscriber_id: 1,
            count: 2,
        });
        assert!(rec.is_inconclusive());
        assert_eq!(
            take_kind(&RuntimeEvent::RunStarted { run_id: run_id() }).as_deref(),
            Some("run_started")
        );
    }

    #[test]
    fn sanitizes_semantic_credentials_in_every_error_text_path() {
        let events = [
            RuntimeEvent::RunFailed {
                error: "request failed: client_secret=run-failed-secret".into(),
                kind: Default::default(),
            },
            RuntimeEvent::RunAborted {
                reason: Some("interrupted: refresh_token: run-aborted-secret".into()),
            },
            RuntimeEvent::HookPanicked {
                hook_name: "audit".into(),
                message: "Authorization: Bearer hook-secret".into(),
            },
            RuntimeEvent::ToolCallFailed {
                tool: "fetch".into(),
                error: ToolError::fatal(
                    "request URL: https://api.example.test/items?client_secret=tool-secret",
                ),
            },
            RuntimeEvent::SubAgentFailed {
                child_run_id: run_id(),
                error: "X-API-Key: subagent-secret".into(),
            },
        ];

        for event in events {
            let outcome = sanitize_runtime_event(&event, 0, 0, RunRelation::default());
            let retained = outcome.event.expect("error event is retained");
            let encoded = serde_json::to_string(&retained.data).expect("event data serializes");
            assert!(encoded.contains(SECRET_REDACTION), "{encoded}");
            for secret in [
                "run-failed-secret",
                "run-aborted-secret",
                "hook-secret",
                "tool-secret",
                "subagent-secret",
            ] {
                assert!(
                    !encoded.contains(secret),
                    "{} leaked from {}",
                    secret,
                    retained.kind
                );
            }
        }
    }

    #[test]
    fn keeps_benign_free_text_unchanged() {
        let message = "retry after timeout; config=release, service: healthy";
        assert_eq!(sanitize_free_text(message), message);
    }

    #[test]
    fn sanitizes_credential_canaries_across_trajectory_error_carriers() {
        let canary = "CREDENTIAL-CANARY-9e97d2";
        let cases = [
            format!("provider failed: --api-key {canary}"),
            format!("model failed: --client-secret {canary}"),
            format!("pre-run failed: https://api.example.test/run?carrier=--api-key%20{canary}"),
            format!("request failed: api_key={canary}"),
            format!("request failed: Authorization: Bearer {canary}"),
            format!("request failed: bearer {canary}"),
        ];

        for input in cases {
            let redacted = sanitize_free_text(&input);
            assert!(
                !text_has_credentials(&redacted),
                "detector still sees credentials in {redacted}"
            );
            assert!(!redacted.contains(canary), "canary leaked in {redacted}");
        }
    }
}
