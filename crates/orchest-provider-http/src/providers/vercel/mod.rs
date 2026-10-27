//! Vercel AI Gateway Chat construction. An OpenAI-compatible multi-provider
//! gateway (`vercel/<vendor>/<model>`, a dynamic model list) over the shared
//! [`ChatAdapter`](crate::chat). Its `reasoning` object and
//! `reasoning` / `reasoning_details` stream match OpenRouter's, so it reuses
//! that profile under its own name
//! (<https://vercel.com/docs/ai-gateway/sdks-and-apis/openai-chat-completions/reasoning>).

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::defaults;
use crate::protocol::ResolvedModel;
use crate::providers::openrouter::OpenRouterProfile;
use crate::ModelError;

/// The profile attached to the Vercel `ProviderEntry`.
pub static VERCEL_PROFILE: OpenRouterProfile = OpenRouterProfile::for_provider("vercel");

fn resolve_api_key(config: &ProviderConfig) -> Result<String, ModelError> {
    config
        .api_key
        .clone()
        .or_else(|| std::env::var(defaults::vercel::API_KEY_ENV).ok())
        .ok_or_else(|| {
            ModelError::internal(
                "AI_GATEWAY_API_KEY not set and no api_key provided",
                "missing_api_key",
            )
        })
}

/// The chat endpoint for a base URL: `…/v1` gains `/chat/completions`, a full
/// endpoint is kept, and a bare host gains `/v1/chat/completions`.
fn normalize_chat_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

fn resolve_url(config_api_url: Option<&str>) -> Result<String, ModelError> {
    let api_url = config_api_url.unwrap_or(defaults::vercel::API_URL);
    if api_url.trim().is_empty() {
        return Err(ModelError::internal(
            "Vercel AI Gateway API URL cannot be empty",
            "invalid_api_url",
        ));
    }
    Ok(normalize_chat_url(api_url))
}

/// ADR-0002 Chat construction: the shared [`ChatAdapter`](crate::chat) at the
/// gateway endpoint with the model id (`<vendor>/<model>`) passed through.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_chat_adapter(
    config: &ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn ChatModel>, ProtocolError> {
    let api_key = resolve_api_key(config).map_err(ProtocolError::from)?;
    let api_url = resolve_url(config.api_url.as_deref()).map_err(ProtocolError::from)?;
    crate::chat::ChatAdapter::build(config, resolved, api_key, api_url)
}

#[cfg(test)]
mod tests;
