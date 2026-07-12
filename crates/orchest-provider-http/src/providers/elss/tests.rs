//! Elss regression tests through the general grammar + protocol-factory path
//! (ADR-0002 slice 009). The old `parse_elss_model` / `resolve_api_url` unit
//! tests are subsumed here: routing is now the general grammar, and URL handling
//! is the general idempotent-append rule.

use crate::{create_adapter_from_config, ModelAdapter, ProviderRuntimeConfig};

fn build(model: &str, api_url: &str) -> Box<dyn ModelAdapter> {
    create_adapter_from_config(ProviderRuntimeConfig {
        model: model.into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some(api_url.into()),
        max_tokens: Some(64),
    })
    .expect("elss resolves through the protocol-factory path")
}

#[test]
fn auto_detect_claude_routes_to_messages() {
    let a = build("elss/claude-sonnet-5", "https://api.elss.ai");
    assert_eq!(a.provider_name(), "anthropic"); // wrapped Messages adapter
    assert_eq!(a.model_name(), "claude-sonnet-5");
}

#[test]
fn auto_detect_non_claude_routes_to_chat() {
    let a = build("elss/gpt-4.1", "https://api.elss.ai");
    assert_eq!(a.provider_name(), "openai"); // wrapped Chat adapter
    assert_eq!(a.model_name(), "gpt-4.1");
}

#[test]
fn provider_alias_anthropic_routes_to_messages() {
    // The `anthropic` segment is an Elss-scoped alias for Messages, even for a
    // non-claude model.
    let a = build("elss/anthropic/gpt-4.1", "https://api.elss.ai");
    assert_eq!(a.provider_name(), "anthropic");
    assert_eq!(a.model_name(), "gpt-4.1");
}

#[test]
fn provider_alias_openai_routes_to_chat() {
    let a = build("elss/openai/claude-sonnet-5", "https://api.elss.ai");
    assert_eq!(a.provider_name(), "openai");
    assert_eq!(a.model_name(), "claude-sonnet-5");
}

#[test]
fn canonical_explicit_protocols_route() {
    assert_eq!(
        build("elss/messages/claude-sonnet-5", "https://api.elss.ai").provider_name(),
        "anthropic"
    );
    assert_eq!(
        build("elss/chat/gpt-4.1", "https://api.elss.ai").provider_name(),
        "openai"
    );
}

#[test]
fn complete_endpoint_api_url_is_respected() {
    // Shipped configs set a complete endpoint; it is used as-is (idempotent).
    let a = build("elss/claude-sonnet-5", "https://api.elss.ai/v1/messages");
    assert_eq!(a.provider_name(), "anthropic");
}

#[test]
fn same_model_two_protocols_user_story() {
    // ADR Problem 4: the same Claude model, reached over either protocol by an
    // explicit segment — Messages (prompt caching) vs Chat (OpenAI tool-use parity).
    let messages = build("elss/messages/claude-sonnet-5", "https://api.elss.ai");
    assert_eq!(messages.provider_name(), "anthropic");
    assert_eq!(messages.model_name(), "claude-sonnet-5");

    let chat = build("elss/chat/claude-sonnet-5", "https://api.elss.ai");
    assert_eq!(chat.provider_name(), "openai");
    assert_eq!(chat.model_name(), "claude-sonnet-5");
}

#[test]
fn aliases_are_provider_scoped_not_global() {
    // OpenRouter declares no aliases, so its `anthropic` segment stays part of the
    // model name — Elss's aliases must not leak globally.
    let normalized =
        crate::normalize_provider_model("openrouter/anthropic/claude-opus-4-8").unwrap();
    assert_eq!(normalized.model, "anthropic/claude-opus-4-8");
    assert_eq!(normalized.protocol, None);
}
