//! Structured error types for tool execution.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidInput,
    NotSupported,
    Transient,
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

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_next_step(mut self, hint: impl Into<String>) -> Self {
        self.next_step = Some(hint.into());
        self
    }
}
