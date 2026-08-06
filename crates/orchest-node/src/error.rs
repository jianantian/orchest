use orchest_protocol::{ModelError, ProtocolError};
use serde::Serialize;
use serde_json::Value;

pub const PROVIDER_ERROR_PREFIX: &str = "__ORCHEST_PROVIDER_ERROR__:";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderErrorData {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_metadata: Option<Value>,
}

impl ProviderErrorData {
    pub fn encoded(self) -> String {
        match serde_json::to_string(&self) {
            Ok(value) => format!("{PROVIDER_ERROR_PREFIX}{value}"),
            Err(_) => self.message,
        }
    }
}

impl From<ModelError> for ProviderErrorData {
    fn from(error: ModelError) -> Self {
        Self {
            message: error.message,
            code: error.code,
            provider: error.provider,
            model: None,
            status: error.status,
            retry_after_secs: error.retry_after_secs,
            upstream: error
                .upstream
                .and_then(|value| serde_json::to_value(value).ok()),
            diagnostic_metadata: None,
        }
    }
}

impl From<ProtocolError> for ProviderErrorData {
    fn from(error: ProtocolError) -> Self {
        Self {
            message: error.message,
            code: serde_json::to_value(error.code)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned)),
            provider: error.provider,
            model: error.model,
            status: error.status,
            retry_after_secs: error.retry_after_secs,
            upstream: error
                .upstream
                .and_then(|value| serde_json::to_value(value).ok()),
            diagnostic_metadata: (!error.diagnostic_metadata.is_null())
                .then_some(error.diagnostic_metadata),
        }
    }
}

pub fn model_error(error: ModelError) -> napi::Error {
    napi::Error::from_reason(ProviderErrorData::from(error).encoded())
}

pub fn protocol_error(error: ProtocolError) -> napi::Error {
    napi::Error::from_reason(ProviderErrorData::from(error).encoded())
}
