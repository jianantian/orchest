//! Structured error types for tool execution.

use serde::{Deserialize, Serialize};

use crate::budget::BudgetUsage;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    InvalidInput,
    NotSupported,
    Transient,
    Ambiguity,
    SpecGap,
    #[default]
    Fatal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum RetryHint {
    Safe,
    Caution,
    #[default]
    Unsafe,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub message: String,
    pub kind: ErrorKind,
    pub retry: RetryHint,
    pub code: Option<String>,
    pub next_step: Option<String>,
    /// Budget an agent-as-tool child run (or any out-of-band worker) consumed
    /// before this error was produced. The runtime folds it into the parent's
    /// `BudgetGuard` on the tool-error path exactly like the success path's
    /// `ToolOutput::Structured.external_usage` — see `run/actor.rs`. `None`
    /// means "nothing out-of-band to account" (the default; ordinary tools
    /// never set this — and it stays off the wire, so tool-result payloads
    /// are byte-identical to before). Only the *terminal* error of a tool
    /// call is folded; a hypothetical retryable tool reporting usage on every
    /// attempt would lose the intermediate attempts' spend (no such producer
    /// exists today — agent-as-tool errors are always `Fatal`/`Unsafe`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_usage: Option<BudgetUsage>,
}

impl ToolError {
    pub fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Fatal,
            retry: RetryHint::Unsafe,
            code: None,
            next_step: None,
            external_usage: None,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::InvalidInput,
            retry: RetryHint::Safe,
            code: None,
            next_step: None,
            external_usage: None,
        }
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Transient,
            retry: RetryHint::Safe,
            code: None,
            next_step: None,
            external_usage: None,
        }
    }

    /// Use when a request allows multiple reasonable interpretations and the
    /// model should ask the caller for clarification instead of retrying.
    pub fn ambiguity(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Ambiguity,
            retry: RetryHint::Unsafe,
            code: None,
            next_step: Some("clarify".to_string()),
            external_usage: None,
        }
    }

    /// Use when the SDK or application contract lacks required behavior and
    /// the model should escalate to a caller or supervising agent.
    pub fn spec_gap(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::SpecGap,
            retry: RetryHint::Unsafe,
            code: None,
            next_step: Some("escalate".to_string()),
            external_usage: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_next_step(mut self, hint: impl Into<String>) -> Self {
        self.next_step = Some(hint.into());
        self
    }

    /// Attach the out-of-band budget consumption that led to this error (see
    /// the [`external_usage`](Self::external_usage) field docs). Used by
    /// agent-as-tool so a failed child run still bills the parent run.
    pub fn with_external_usage(mut self, usage: BudgetUsage) -> Self {
        self.external_usage = Some(usage);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorKind, RetryHint, ToolError};

    #[test]
    fn tool_error_ambiguity_sets_clarification_contract() {
        let error = ToolError::ambiguity("choose a target");

        assert_eq!(error.kind, ErrorKind::Ambiguity);
        assert_eq!(error.retry, RetryHint::Unsafe);
        assert_eq!(error.next_step.as_deref(), Some("clarify"));
    }

    #[test]
    fn tool_error_spec_gap_sets_escalation_contract() {
        let error = ToolError::spec_gap("missing policy");

        assert_eq!(error.kind, ErrorKind::SpecGap);
        assert_eq!(error.retry, RetryHint::Unsafe);
        assert_eq!(error.next_step.as_deref(), Some("escalate"));
    }

    #[test]
    fn tool_error_serializes_structured_fields() {
        let error = ToolError::ambiguity("unclear").with_code("AMBIGUOUS_TARGET");
        let value = serde_json::to_value(&error).expect("ToolError serializes");

        assert_eq!(value["message"], "unclear");
        assert_eq!(value["kind"], "Ambiguity");
        assert_eq!(value["retry"], "Unsafe");
        assert_eq!(value["code"], "AMBIGUOUS_TARGET");
        assert_eq!(value["next_step"], "clarify");

        let round_trip: ToolError = serde_json::from_value(value).expect("ToolError deserializes");
        assert_eq!(round_trip.kind, ErrorKind::Ambiguity);
    }

    /// Payloads emitted before the `external_usage` field existed must still
    /// deserialize (the field defaults to `None`).
    #[test]
    fn tool_error_deserializes_pre_external_usage_payloads() {
        let legacy = serde_json::json!({
            "message": "boom",
            "kind": "Fatal",
            "retry": "Unsafe",
            "code": null,
            "next_step": null,
        });
        let error: ToolError = serde_json::from_value(legacy).expect("legacy payload");
        assert!(error.external_usage.is_none());
    }
}
