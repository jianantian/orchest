//! DeepSeek Chat profile + construction (ADR-0002 Phase 3). OpenAI-compatible
//! Chat over the shared [`ChatAdapter`](crate::chat); the reasoning-dialect
//! deviation lives in [`DeepSeekProfile`].

mod profile;
mod request;

use std::env;

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::defaults;
use crate::protocol::ResolvedModel;
use crate::ModelError;

pub use profile::{DeepSeekProfile, DEEPSEEK_PROFILE};

fn resolve_api_key(config: &ProviderConfig) -> Result<String, ModelError> {
    config
        .api_key
        .clone()
        .or_else(|| env::var("DEEPSEEK_API_KEY").ok())
        .ok_or_else(|| {
            ModelError::internal(
                "DEEPSEEK_API_KEY not set and no api_key provided",
                "missing_api_key",
            )
        })
}

fn resolve_url(config_api_url: Option<&str>) -> Result<String, ModelError> {
    let api_url = config_api_url
        .map(String::from)
        .unwrap_or_else(|| defaults::deepseek::API_URL.to_string());
    if api_url.trim().is_empty() {
        return Err(ModelError::internal(
            "DeepSeek API URL cannot be empty",
            "invalid_api_url",
        ));
    }
    Ok(request::normalize_chat_url(&api_url))
}

/// ADR-0002 Chat construction: the shared [`ChatAdapter`](crate::chat) with
/// DeepSeek's resolved endpoint + profile.
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
