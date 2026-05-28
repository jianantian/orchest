use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            upstream: None,
        }
    }
}
