use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsrErrorCode {
    MissingApiKey,
    InvalidApiKey,
    UnknownProvider,
    UnknownModel,
    NoMatchingProvider,
    UnsupportedOperation,
    UnsupportedOption,
    UnsupportedLanguage,
    UnsupportedAudioFormat,
    InvalidAudio,
    InvalidRequest,
    ProviderHttpError,
    ProviderStreamError,
    ProviderTaskFailed,
    Timeout,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrError {
    pub message: String,
    pub code: AsrErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_body: Option<Value>,
    #[serde(default)]
    pub diagnostic_metadata: Value,
}

impl AsrError {
    pub fn new(code: AsrErrorCode, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code,
            model: None,
            status: None,
            upstream_code: None,
            upstream_message: None,
            upstream_body: None,
            diagnostic_metadata: Value::Null,
        }
    }

    pub fn unsupported_operation() -> Self {
        Self::new(
            AsrErrorCode::UnsupportedOperation,
            "operation not supported by this provider",
        )
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_upstream(
        mut self,
        status: Option<u16>,
        upstream_code: Option<String>,
        upstream_message: Option<String>,
        upstream_body: Option<Value>,
    ) -> Self {
        self.status = status;
        self.upstream_code = upstream_code;
        self.upstream_message = upstream_message;
        self.upstream_body = upstream_body.map(|mut b| {
            redact_secrets(&mut b);
            b
        });
        self
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("key")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("credential")
        || lower.contains("authorization")
}

pub fn redact_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_secret_key(k) {
                    *v = Value::String("[REDACTED]".into());
                } else {
                    redact_secrets(v);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_secrets(item);
            }
        }
        _ => {}
    }
}

impl fmt::Display for AsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for AsrError {}
