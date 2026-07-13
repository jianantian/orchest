//! OpenRouter Chat profile + construction (ADR-0002 Phase 3). OpenAI-compatible
//! multi-provider gateway over the shared [`ChatAdapter`](crate::chat); deviations
//! live in [`OpenRouterProfile`](profile::OpenRouterProfile), and the routing headers ride on the entry
//! (`HeaderValue::Env`, resolved by the shared core).

mod profile;
mod request;

use std::env;

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::defaults;
use crate::protocol::ResolvedModel;
use crate::ModelError;

pub use profile::OPENROUTER_PROFILE;

fn resolve_api_key(config: &ProviderConfig) -> Result<String, ModelError> {
    config
        .api_key
        .clone()
        .or_else(|| env::var(defaults::openrouter::API_KEY_ENV).ok())
        .ok_or_else(|| {
            ModelError::internal(
                "OPENROUTER_API_KEY not set and no api_key provided",
                "missing_api_key",
            )
        })
}

fn resolve_url(config_api_url: Option<&str>) -> Result<String, ModelError> {
    let api_url = config_api_url
        .map(String::from)
        .unwrap_or_else(|| defaults::openrouter::API_URL.to_string());
    if api_url.trim().is_empty() {
        return Err(ModelError::internal(
            "OpenRouter API URL cannot be empty",
            "invalid_api_url",
        ));
    }
    Ok(request::normalize_chat_url(&api_url))
}

/// ADR-0002 Chat construction: the shared [`ChatAdapter`](crate::chat) with
/// OpenRouter's resolved endpoint + profile (routing headers resolved from the
/// entry by the core).
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
