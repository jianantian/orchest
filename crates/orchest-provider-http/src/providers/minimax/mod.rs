//! Minimax Messages profile + construction (ADR-0002 Phase 3). Anthropic-Messages
//! compatible (`POST /anthropic/v1/messages`) over the shared
//! `MessagesAdapter`; its divergence (native roles, real
//! multimodal blocks, `Bearer` auth, capability facts) lives in [`MinimaxProfile`](profile::MinimaxProfile).

mod profile;
mod request;

use std::env;

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::defaults;
use crate::protocol::ResolvedModel;
use crate::ModelError;

pub use profile::MINIMAX_PROFILE;
use request::normalize_messages_url;

fn resolve_api_key(config: &ProviderConfig) -> Result<String, ModelError> {
    config
        .api_key
        .clone()
        .or_else(|| env::var(defaults::minimax::API_KEY_ENV).ok())
        .ok_or_else(|| {
            ModelError::internal(
                "MINIMAX_API_KEY not set and no api_key provided",
                "missing_api_key",
            )
        })
}

fn resolve_url(config_api_url: Option<&str>) -> Result<String, ModelError> {
    let api_url = config_api_url
        .map(String::from)
        .or_else(|| {
            env::var(defaults::minimax::API_URL_ENV)
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| defaults::minimax::API_URL.to_string());
    if api_url.trim().is_empty() {
        return Err(ModelError::internal(
            "Minimax API URL cannot be empty",
            "invalid_api_url",
        ));
    }
    Ok(normalize_messages_url(&api_url))
}

/// ADR-0002 Messages construction: the shared `MessagesAdapter`
/// with Minimax's resolved endpoint + profile.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_messages_adapter(
    config: &ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn ChatModel>, ProtocolError> {
    let api_key = resolve_api_key(config).map_err(ProtocolError::from)?;
    let api_url = resolve_url(config.api_url.as_deref()).map_err(ProtocolError::from)?;
    crate::messages::MessagesAdapter::build(config, resolved, api_key, api_url)
}

#[cfg(test)]
mod tests;
