//! Elss API gateway — a **pure `ProviderEntry`** (ADR-0002).
//!
//! Elss (<https://elss.ai>) is an aggregator exposing one key for Anthropic
//! (Messages) and OpenAI (Chat). It has no adapter code of its own: the grammar
//! resolves the protocol, and this ctor builds the canonical adapter for that
//! protocol pointed at Elss's base URL. Chat routes through the shared
//! [`ChatAdapter`](crate::chat) as the OpenAI dialect; Messages through the shared
//! `MessagesAdapter` as the Anthropic dialect.

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::protocol::{provider_entry, Protocol, ResolvedModel};

/// Elss's base URL: the user-supplied `api_url`, else `ELSS_API_URL`, else the
/// default. The wrapped adapter appends the protocol's canonical path.
fn elss_base_url(user_api_url: Option<&str>) -> String {
    if let Some(url) = user_api_url {
        return url.to_string();
    }
    std::env::var(crate::defaults::elss::API_URL_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| crate::defaults::elss::API_URL.to_string())
}

/// ADR-0002 construction: dispatch on the resolved **protocol** (not a provider
/// name — protocol dispatch is permitted). Messages builds the shared
/// MessagesAdapter with Anthropic's entry + profile, Chat the shared ChatAdapter
/// as the OpenAI dialect, both at Elss's base URL.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_adapter(
    config: &ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn ChatModel>, ProtocolError> {
    let base = elss_base_url(config.api_url.as_deref());
    match resolved.protocol {
        Protocol::Messages => {
            // Elss/messages is the Anthropic dialect: build the shared
            // MessagesAdapter with Anthropic's entry + profile, at Elss's endpoint.
            let anthropic_entry =
                provider_entry("anthropic").expect("anthropic entry is registered");
            let messages_resolved = ResolvedModel {
                provider: anthropic_entry,
                protocol: Protocol::Messages,
                model: resolved.model,
                catalog: resolved.catalog,
            };
            let mut messages_config = config.clone();
            messages_config.api_url = Some(base);
            crate::providers::anthropic::build_messages_adapter(
                &messages_config,
                &messages_resolved,
            )
        }
        Protocol::Chat => {
            // Elss/chat is the OpenAI dialect: build the shared ChatAdapter with
            // OpenAI's entry + profile, pointed at Elss's endpoint.
            let openai_entry = provider_entry("openai").expect("openai entry is registered");
            let chat_resolved = ResolvedModel {
                provider: openai_entry,
                protocol: Protocol::Chat,
                model: resolved.model,
                catalog: resolved.catalog,
            };
            let mut chat_config = config.clone();
            chat_config.api_url = Some(base);
            crate::providers::openai::build_chat_adapter(&chat_config, &chat_resolved)
        }
        Protocol::Responses => Err(ProtocolError::internal_err(
            "elss does not support the Responses protocol",
        )),
    }
}

#[cfg(test)]
mod tests;
