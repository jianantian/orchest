//! Error types: the legacy chat `ModelError` and the unified `ProtocolError`
//! (v0.9.12 provider unification).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpstreamErrorDetail {
    pub code: Option<String>,
    pub message: Option<String>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub message: String,
    pub code: Option<String>,
    pub provider: Option<String>,
    pub status: Option<u16>,
    /// Value of the `Retry-After` HTTP header in seconds, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Arc<UpstreamErrorDetail>>,
}

impl ModelError {
    pub fn internal(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: Some(code.into()),
            provider: None,
            status: None,
            retry_after_secs: None,
            upstream: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Unified protocol error (v0.9.12 provider unification)
// ---------------------------------------------------------------------------

/// Stable error classification, unifying `ModelError`/`AsrError`/`TtsError`/
/// `RealtimeError` codes (PRD §From Here To There). The satellite error code
/// enums are near-identical supersets; this is their union. Application control
/// flow should switch on this, not on message strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCode {
    // auth
    MissingApiKey,
    InvalidApiKey,
    // routing / identity
    UnknownProvider,
    UnknownModel,
    NoMatchingProvider,
    // capability / validation
    UnsupportedOperation,
    UnsupportedOption,
    UnsupportedLanguage,
    UnsupportedAudioFormat,
    InvalidAudio,
    InvalidRequest,
    /// A capability returned data that violates its response contract.
    InvalidResponse,
    // chat-side request outcomes
    ContentFilter,
    ContextWindowExceeded,
    // transport / provider
    ProviderHttpError,
    ProviderStreamError,
    ProviderTaskFailed,
    Timeout,
    Cancelled,
    // catch-alls
    Internal,
    Other,
}

/// The one error every capability returns once providers are migrated onto the
/// spine. Structurally the superset of the four legacy errors: it keeps
/// `ModelError`'s `retry_after_secs` and `Arc<UpstreamErrorDetail>`, and the
/// satellite errors' `model` + `diagnostic_metadata`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, thiserror::Error)]
#[error("[{code:?}] {message}")]
pub struct ProtocolError {
    pub message: String,
    pub code: ErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Value of the `Retry-After` HTTP header in seconds, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Arc<UpstreamErrorDetail>>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub diagnostic_metadata: Value,
}

impl ProtocolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code,
            provider: None,
            model: None,
            status: None,
            retry_after_secs: None,
            upstream: None,
            diagnostic_metadata: Value::Null,
        }
    }

    pub fn internal_err(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    #[must_use]
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }
}

/// Bridge the legacy chat error into the unified error. The satellite errors
/// (`AsrError`/`TtsError`/`RealtimeError`) gain symmetric `From` shims when their
/// crates start depending on the spine (Issues 006/007).
impl From<ModelError> for ProtocolError {
    fn from(e: ModelError) -> Self {
        Self {
            message: e.message,
            code: ErrorCode::Other,
            provider: e.provider,
            model: None,
            status: e.status,
            retry_after_secs: e.retry_after_secs,
            upstream: e.upstream,
            diagnostic_metadata: Value::Null,
        }
    }
}

impl From<ProtocolError> for ModelError {
    fn from(e: ProtocolError) -> Self {
        Self {
            message: e.message,
            code: Some(format!("{:?}", e.code)),
            provider: e.provider,
            status: e.status,
            retry_after_secs: e.retry_after_secs,
            upstream: e.upstream,
        }
    }
}
