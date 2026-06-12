use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsErrorCode {
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
pub struct TtsError {
    pub message: String,
    pub code: TtsErrorCode,
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

impl TtsError {
    pub fn new(code: TtsErrorCode, message: impl Into<String>) -> Self {
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
            TtsErrorCode::UnsupportedOperation,
            "operation not supported by this provider",
        )
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.diagnostic_metadata = metadata;
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
        self.upstream_body = upstream_body.map(|mut body| {
            redact_secrets(&mut body);
            body
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
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    *value = Value::String("[REDACTED]".to_owned());
                } else {
                    redact_secrets(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_secrets(item);
            }
        }
        Value::String(text) if text.to_ascii_lowercase().contains("authorization:") => {
            *text = "[REDACTED]".to_owned();
        }
        _ => {}
    }
}

impl fmt::Display for TtsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for TtsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_upstream_redacts_secret_fields_recursively() {
        let error = TtsError::new(TtsErrorCode::ProviderHttpError, "provider failed")
            .with_upstream(
                Some(401),
                Some("Unauthorized".to_owned()),
                None,
                Some(serde_json::json!({
                    "api_key": "secret",
                    "nested": {
                        "Authorization": "Bearer token"
                    },
                    "items": [
                        {"access_token": "token"}
                    ],
                    "message": "safe"
                })),
            );

        let body = error.upstream_body.unwrap();
        assert_eq!(body["api_key"], "[REDACTED]");
        assert_eq!(body["nested"]["Authorization"], "[REDACTED]");
        assert_eq!(body["items"][0]["access_token"], "[REDACTED]");
        assert_eq!(body["message"], "safe");
    }
}
