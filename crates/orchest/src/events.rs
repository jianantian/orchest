//! Runtime event types emitted during agent execution.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::model::{ModelStreamChunk, OptionAdjustment, StopReason, TokenUsage};
use crate::run::RunId;
use crate::tool::async_job::JobStatus;
use crate::tool::{ToolCall, ToolError, ToolMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ApprovalContext {
    #[default]
    InitialToolCall,
    CommitToolCall {
        draft_tool: String,
    },
    RetryAfterFailure {
        attempt: u32,
        previous_error: ToolError,
    },
}

/// An event emitted on the run's event stream: run lifecycle, model calls,
/// tool calls, approvals, budget, sub-agents, and steering. Consumers receive
/// these from the `EventReceiver` returned by `AgentRun::start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    RunStarted {
        run_id: RunId,
    },

    ModelCallStarted {
        step: u32,
    },
    ModelStreamChunk {
        delta: ModelStreamChunk,
    },
    ModelCallCompleted {
        tokens: TokenUsage,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        option_adjustments: Vec<OptionAdjustment>,
    },
    /// A model call failed with a retryable error and will be retried after
    /// `next_delay`. Subscribers that accumulated partial `ModelStreamChunk`s
    /// from the interrupted attempt must drop that buffer: the retry restarts
    /// the stream from the beginning.
    ModelRetry {
        attempt: u32,
        error: String,
        next_delay: Duration,
    },

    ToolCallStarted {
        tool: String,
        metadata: ToolMetadata,
        input: Value,
    },
    ToolCallUpdate {
        tool: String,
        tool_call_id: String,
        partial: Value,
    },
    ToolCallCompleted {
        tool: String,
        output: Value,
        duration: Duration,
    },
    ToolCallFailed {
        tool: String,
        error: ToolError,
    },
    ToolCallRetry {
        tool: String,
        attempt: u32,
        previous_error: ToolError,
        next_delay: Duration,
    },
    ToolCallBatchStarted {
        batch_id: String,
        tool_count: usize,
    },
    ToolCallBatchItemStarted {
        batch_id: String,
        tool: String,
        requested_order: usize,
    },
    ToolCallBatchItemCompleted {
        batch_id: String,
        tool: String,
        requested_order: usize,
        completion_order: usize,
    },

    AsyncToolStarted {
        tool: String,
        job_id: String,
    },
    AsyncToolProgress {
        tool: String,
        job_id: String,
        status: JobStatus,
    },
    AsyncToolCompleted {
        tool: String,
        job_id: String,
        output: Value,
        elapsed: Duration,
    },

    SkillContentRead {
        skill_name: String,
        file: String,
        tokens: u32,
    },

    ApprovalRequested {
        tool_call: ToolCall,
        #[serde(default)]
        context: ApprovalContext,
    },
    ApprovalGranted {
        tool_call: ToolCall,
        #[serde(default)]
        context: ApprovalContext,
    },
    ApprovalDenied {
        tool_call: ToolCall,
        #[serde(default)]
        context: ApprovalContext,
    },

    BudgetWarning {
        used: BudgetUsage,
        limit: BudgetConfig,
    },
    RuntimeWarning {
        message: String,
    },
    SkillMissingCapabilities {
        skill_name: String,
    },
    SkillLoadWarning {
        path: String,
        reason: String,
    },
    ContextCompacted {
        removed_messages: usize,
        summary_tokens: u32,
    },

    SubAgentStarted {
        parent_run_id: RunId,
        child_run_id: RunId,
        config_summary: Value,
    },
    SubAgentCompleted {
        child_run_id: RunId,
        output: Value,
        budget_used: BudgetUsage,
    },
    SubAgentFailed {
        child_run_id: RunId,
        error: String,
    },

    ChildRunEvent {
        child_run_id: RunId,
        run_depth: u32,
        event: Box<RuntimeEvent>,
    },
    SubAgentEvent {
        parent_run_id: RunId,
        child_run_id: RunId,
        event: Box<RuntimeEvent>,
    },

    HookPanicked {
        hook_name: String,
        message: String,
    },

    AgentUpdated {
        previous_agent: String,
        new_agent: String,
    },

    EventsDropped {
        subscriber_id: u64,
        count: u64,
    },

    RunRestarted {
        attempt: u32,
    },

    /// The run finished normally; `output` is the final assistant text.
    ///
    /// `stop_reason` carries the model's stop reason for the completing turn:
    /// [`StopReason::EndTurn`] means the output is complete, while
    /// [`StopReason::MaxTokens`] means the model hit the token cap and
    /// `output` is **truncated**. Consumers must read this marker before
    /// treating `output` as final: on truncation, decide whether to continue
    /// generation (ask the model to keep writing), retry with a larger token
    /// budget, or surface an error. Persisting truncated output as-is (e.g.
    /// half-written HTML) is the failure mode this field exists to prevent.
    RunCompleted {
        output: Value,
        /// Why the completing model call stopped. Defaults to
        /// [`StopReason::EndTurn`] when deserializing events emitted before
        /// this field existed (they only distinguished normal completion).
        #[serde(default = "default_run_completed_stop_reason")]
        stop_reason: StopReason,
    },
    RunFailed {
        error: String,
    },
    RunAborted {
        reason: Option<String>,
    },
}

/// Serde default for `RunCompleted::stop_reason` on events serialized before
/// the field existed: those only distinguished normal completion, so
/// `EndTurn` (not truncated) is the faithful reading.
fn default_run_completed_stop_reason() -> StopReason {
    StopReason::EndTurn
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn skill_load_warning_round_trips_through_serde() {
        let event = RuntimeEvent::SkillLoadWarning {
            path: "skills/bad/SKILL.md".to_string(),
            reason: "invalid frontmatter YAML: missing field `name`".to_string(),
        };

        let serialized = serde_json::to_string(&event).expect("serialize event");
        let deserialized: RuntimeEvent =
            serde_json::from_str(&serialized).expect("deserialize event");

        match deserialized {
            RuntimeEvent::SkillLoadWarning { path, reason } => {
                assert_eq!(path, "skills/bad/SKILL.md");
                assert!(reason.contains("invalid frontmatter YAML"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn sub_agent_event_round_trips_through_serde() {
        let parent_run_id = RunId::new();
        let child_run_id = RunId::new();
        let event = RuntimeEvent::SubAgentEvent {
            parent_run_id,
            child_run_id,
            event: Box::new(RuntimeEvent::RunCompleted {
                output: json!("done"),
                stop_reason: StopReason::EndTurn,
            }),
        };

        let serialized = serde_json::to_string(&event).expect("serialize event");
        let deserialized: RuntimeEvent =
            serde_json::from_str(&serialized).expect("deserialize event");

        match deserialized {
            RuntimeEvent::SubAgentEvent {
                parent_run_id: parent,
                child_run_id: child,
                event,
            } => {
                assert_eq!(parent, parent_run_id);
                assert_eq!(child, child_run_id);
                assert!(matches!(
                    event.as_ref(),
                    RuntimeEvent::RunCompleted { output, .. } if output == "done"
                ));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn run_completed_stop_reason_round_trips_through_serde() {
        let event = RuntimeEvent::RunCompleted {
            output: json!("cut off"),
            stop_reason: StopReason::MaxTokens,
        };

        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(
            value,
            json!({"RunCompleted": {"output": "cut off", "stop_reason": "MaxTokens"}})
        );
        let deserialized: RuntimeEvent = serde_json::from_value(value).expect("deserialize event");
        assert!(matches!(
            deserialized,
            RuntimeEvent::RunCompleted {
                stop_reason: StopReason::MaxTokens,
                ..
            }
        ));
    }

    #[test]
    fn run_completed_without_stop_reason_deserializes_as_end_turn() {
        // Events serialized before the stop_reason field existed carry only
        // `output`; they must still deserialize (serde default).
        let legacy = json!({"RunCompleted": {"output": "done"}});
        let event: RuntimeEvent = serde_json::from_value(legacy).expect("deserialize legacy event");

        match event {
            RuntimeEvent::RunCompleted {
                output,
                stop_reason,
            } => {
                assert_eq!(output, json!("done"));
                assert_eq!(stop_reason, StopReason::EndTurn);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
