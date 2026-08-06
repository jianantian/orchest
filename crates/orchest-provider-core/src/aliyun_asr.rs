//! Shared validation for Aliyun HTTP and WebSocket ASR dialects.

use orchest_protocol::{ErrorCode, ProtocolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: ContextRole,
    pub text: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries provider diagnostics by workspace convention
pub fn parse_context(options: &Value) -> Result<Vec<ContextMessage>, ProtocolError> {
    let Some(raw) = options.get("context") else {
        return Ok(Vec::new());
    };
    let messages: Vec<ContextMessage> = serde_json::from_value(raw.clone()).map_err(|error| {
        ProtocolError::new(
            ErrorCode::InvalidRequest,
            format!("invalid Aliyun ASR context: {error}"),
        )
    })?;

    let mut user_count = 0usize;
    let mut assistant_count = 0usize;
    let mut pending_user_chars = None;
    for message in &messages {
        if message.text.is_empty() {
            return Err(invalid_context("context text cannot be empty"));
        }
        match message.role {
            ContextRole::User => {
                if pending_user_chars.is_some() {
                    return Err(invalid_context(
                        "each context round must place assistant after user",
                    ));
                }
                user_count += 1;
                pending_user_chars = Some(message.text.chars().count());
            }
            ContextRole::Assistant => {
                let Some(user_chars) = pending_user_chars.take() else {
                    return Err(invalid_context(
                        "assistant context must follow its user context",
                    ));
                };
                assistant_count += 1;
                if user_chars + message.text.chars().count() > 400 {
                    return Err(invalid_context(
                        "one context round cannot exceed 400 characters",
                    ));
                }
            }
        }
    }
    if user_count > 5 || assistant_count > 5 {
        return Err(invalid_context(
            "Aliyun ASR context allows at most 5 messages per role",
        ));
    }
    if pending_user_chars.is_some_and(|count| count > 400) {
        return Err(invalid_context(
            "one context round cannot exceed 400 characters",
        ));
    }
    Ok(messages)
}

fn invalid_context(message: &str) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidRequest, message)
}
