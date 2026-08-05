//! Provider-neutral atomic capabilities that do not start an agent run.

use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelError, ModelResponse, RequestOptions, ResponseFormat,
    Role, StopReason,
};
use crate::run::retry::{classify, compute_delay, should_retry};
use crate::run::RetryPolicy;

/// Inputs for one provider-neutral chat completion.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: Option<String>,
    pub user: String,
    pub options: RequestOptions,
    pub retry_policy: Option<RetryPolicy>,
}

/// Complete one system/user turn without tools, streaming, or an agent run.
pub async fn complete(
    model: &dyn ModelAdapter,
    request: CompletionRequest,
) -> Result<String, ModelError> {
    let messages = completion_messages(request.system, request.user);
    let mut attempt = 0;

    loop {
        match model.complete(&messages, &[], &request.options, None).await {
            Ok(response) => {
                return validate_response(
                    response,
                    request.options.response_format,
                    model.provider_name(),
                )
            }
            Err(error) => {
                if !should_retry(&classify(&error), attempt, &request.retry_policy) {
                    return Err(error);
                }
                let Some(policy) = request.retry_policy.as_ref() else {
                    return Err(error);
                };
                tokio::time::sleep(compute_delay(attempt, &error, policy)).await;
                attempt += 1;
            }
        }
    }
}

fn completion_messages(system: Option<String>, user: String) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2);
    if let Some(system) = system.filter(|value| !value.is_empty()) {
        messages.push(Message {
            role: Role::System,
            content: vec![ContentBlock::Text(system)],
        });
    }
    messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text(user)],
    });
    messages
}

fn validate_response(
    response: ModelResponse,
    format: ResponseFormat,
    provider: &str,
) -> Result<String, ModelError> {
    if !matches!(
        response.stop_reason,
        StopReason::EndTurn | StopReason::StopSequence
    ) {
        return Err(atomic_error(
            provider,
            "atomic_incomplete",
            format!(
                "atomic completion ended with non-success stop reason {:?}",
                response.stop_reason
            ),
        ));
    }

    let text = response
        .content
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text),
            _ => None,
        })
        .collect::<String>();

    if format == ResponseFormat::JsonObject {
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
            atomic_error(
                provider,
                "invalid_json_object",
                format!("provider returned invalid JSON object: {error}"),
            )
        })?;
        if !value.is_object() {
            return Err(atomic_error(
                provider,
                "invalid_json_object",
                "provider returned JSON whose top-level value is not an object",
            ));
        }
    }

    Ok(text)
}

fn atomic_error(provider: &str, code: &str, message: impl Into<String>) -> ModelError {
    let mut error = ModelError::internal(message, code);
    error.provider = Some(provider.to_string());
    error
}
