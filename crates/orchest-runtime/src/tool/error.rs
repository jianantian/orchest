//! Structured error types for tool execution.

use serde::{Deserialize, Serialize};

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
}

impl ToolError {
    pub fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Fatal,
            retry: RetryHint::Unsafe,
            code: None,
            next_step: None,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::InvalidInput,
            retry: RetryHint::Safe,
            code: None,
            next_step: None,
        }
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Transient,
            retry: RetryHint::Safe,
            code: None,
            next_step: None,
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
}
