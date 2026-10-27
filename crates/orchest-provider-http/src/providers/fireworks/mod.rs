//! Fireworks Chat construction. OpenAI-compatible inference over the shared
//! [`ChatAdapter`](crate::chat); the model list changes often, so it is a
//! dynamic provider. `fireworks/<name>` targets the Fireworks-hosted
//! `accounts/fireworks/models/<name>`; an id that already starts with
//! `accounts/` is passed through. Deviations live in [`FireworksProfile`].

mod profile;

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::defaults;
use crate::protocol::ResolvedModel;
use crate::ModelError;

pub use profile::FIREWORKS_PROFILE;

fn resolve_api_key(config: &ProviderConfig) -> Result<String, ModelError> {
    config
        .api_key
        .clone()
        .or_else(|| std::env::var(defaults::fireworks::API_KEY_ENV).ok())
        .ok_or_else(|| {
            ModelError::internal(
                "FIREWORKS_API_KEY not set and no api_key provided",
                "missing_api_key",
            )
        })
}

/// The chat endpoint for a base URL: `…/v1` gains `/chat/completions`, a full
/// endpoint is kept, and anything else gains `/v1/chat/completions`.
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
    let api_url = config_api_url.unwrap_or(defaults::fireworks::API_URL);
    if api_url.trim().is_empty() {
        return Err(ModelError::internal(
            "Fireworks API URL cannot be empty",
            "invalid_api_url",
        ));
    }
    Ok(normalize_chat_url(api_url))
}

/// The upstream model id: a bare name is a Fireworks-hosted model
/// (`accounts/fireworks/models/<name>`); a full `accounts/…` path is kept.
pub fn upstream_model(model: &str) -> String {
    if model.starts_with("accounts/") {
        model.to_string()
    } else {
        format!("{}{model}", defaults::fireworks::MODEL_PREFIX)
    }
}

/// ADR-0002 Chat construction: the shared [`ChatAdapter`](crate::chat) at the
/// Fireworks endpoint, with the model id mapped by [`upstream_model`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_chat_adapter(
    config: &ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn ChatModel>, ProtocolError> {
    let api_key = resolve_api_key(config).map_err(ProtocolError::from)?;
    let api_url = resolve_url(config.api_url.as_deref()).map_err(ProtocolError::from)?;
    let model = upstream_model(resolved.model);
    let resolved = ResolvedModel {
        provider: resolved.provider,
        protocol: resolved.protocol,
        model: &model,
        catalog: None,
    };
    crate::chat::ChatAdapter::build(config, &resolved, api_key, api_url)
}

#[cfg(test)]
mod tests;
