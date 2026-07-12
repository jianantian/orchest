//! Elss API gateway — a **pure `ProviderEntry`** (ADR-0002 slice 009).
//!
//! Elss (<https://elss.ai>) is an API aggregator exposing a unified key for
//! Anthropic (`/v1/messages`) and OpenAI (`/v1/chat/completions`). It has **no
//! adapter code of its own**: the model-string grammar (slice 008) resolves the
//! protocol, and this ctor wraps the canonical Anthropic/OpenAI adapter for that
//! protocol with Elss's base URL. The previous `parse_elss_model` /
//! `resolve_api_url` routing is subsumed by the general grammar + URL rule.
//!
//! Model-string routing (all handled by the general grammar):
//! - `elss/claude-sonnet-5` → Messages (auto-detected: claude-* prefix)
//! - `elss/gpt-4.1` → Chat (auto-detected)
//! - `elss/anthropic/claude-sonnet-5` → Messages (provider alias `anthropic`)
//! - `elss/openai/gpt-4.1` → Chat (provider alias `openai`)
//! - `elss/messages/…` / `elss/chat/…` → canonical explicit protocol

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::protocol::{Protocol, ResolvedModel};
use crate::providers::anthropic::{AnthropicAdapter, AnthropicConfig};
use crate::providers::openai::{OpenAiAdapter, OpenAiConfig};

/// Elss's base URL: the user-supplied `api_url`, else `ELSS_API_URL`, else the
/// default. It is a base URL — the wrapped adapter's `normalize_*_url` appends
/// the protocol's canonical path (idempotently for complete endpoints).
fn elss_base_url(user_api_url: Option<&str>) -> String {
    if let Some(url) = user_api_url {
        return url.to_string();
    }
    std::env::var(crate::defaults::elss::API_URL_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| crate::defaults::elss::API_URL.to_string())
}

/// ADR-0002 protocol-factory path (slice 009). Referenced as data by the Elss
/// `ProviderEntry.build_adapter`; both `ChatProtocolFactory` and
/// `MessagesProtocolFactory` reach it. Dispatch is on the resolved **protocol**
/// (not a provider name — ADR rule 1 permits protocol dispatch): Messages wraps
/// the Anthropic adapter, Chat wraps the OpenAI adapter, both pointed at Elss's
/// base URL.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_adapter(
    config: &ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn ChatModel>, ProtocolError> {
    let base = elss_base_url(config.api_url.as_deref());
    let max_tokens = config.max_tokens.unwrap_or(crate::defaults::MAX_TOKENS);
    let model = resolved.model.to_string();

    let adapter: Box<dyn ChatModel> = match resolved.protocol {
        Protocol::Messages => Box::new(
            AnthropicAdapter::from_config(AnthropicConfig {
                model,
                max_tokens,
                api_key: config.api_key.clone(),
                api_url: Some(base),
            })
            .map_err(ProtocolError::from)?,
        ),
        Protocol::Chat => Box::new(
            OpenAiAdapter::from_config(OpenAiConfig {
                model,
                max_tokens,
                api_key: config.api_key.clone(),
                api_url: Some(base),
            })
            .map_err(ProtocolError::from)?,
        ),
        Protocol::Responses => {
            return Err(ProtocolError::internal_err(
                "elss does not support the Responses protocol",
            ));
        }
    };
    Ok(adapter)
}

#[cfg(test)]
mod tests;
