use orchest_provider_core::registry::ProviderConfig;

use super::*;
use crate::chat::ChatAdapter;
use crate::providers::anthropic::test_util::*;
use crate::test_env::{lock_env, EnvVarGuard};
use crate::{
    create_adapter_from_config, normalize_provider_model, ProviderRuntimeConfig, RequestOptions,
    ThinkingLevel,
};

fn make_adapter(api_url: &str) -> ChatAdapter {
    ChatAdapter::for_test("vercel", "openai/gpt-5.4", api_url, 4096)
}

#[test]
fn default_api_url() {
    assert_eq!(
        resolve_url(None).unwrap(),
        "https://ai-gateway.vercel.sh/v1/chat/completions"
    );
}

#[test]
fn api_url_is_normalized() {
    for (input, expected) in [
        (
            "https://ai-gateway.vercel.sh/v1/",
            "https://ai-gateway.vercel.sh/v1/chat/completions",
        ),
        (
            "https://ai-gateway.vercel.sh/v1/chat/completions",
            "https://ai-gateway.vercel.sh/v1/chat/completions",
        ),
        (
            "https://proxy.example.com",
            "https://proxy.example.com/v1/chat/completions",
        ),
    ] {
        assert_eq!(resolve_url(Some(input)).unwrap(), expected);
    }
    assert!(resolve_url(Some("  ")).is_err());
}

#[test]
fn explicit_api_key_wins_and_env_is_the_fallback() {
    let _lock = lock_env();
    let _key = EnvVarGuard::set("AI_GATEWAY_API_KEY", "env-key");
    let explicit = ProviderConfig::new("vercel", "openai/gpt-5.4").with_api_key("explicit");
    assert_eq!(resolve_api_key(&explicit).unwrap(), "explicit");
    let from_env = ProviderConfig::new("vercel", "openai/gpt-5.4");
    assert_eq!(resolve_api_key(&from_env).unwrap(), "env-key");
}

#[test]
fn missing_api_key_is_reported() {
    let _lock = lock_env();
    let _key = EnvVarGuard::remove("AI_GATEWAY_API_KEY");
    let error = create_adapter_from_config(ProviderRuntimeConfig {
        model: "vercel/openai/gpt-5.4".into(),
        api_key: None,
        api_key_env: None,
        api_url: None,
        max_tokens: None,
    })
    .err()
    .expect("missing key errors");
    assert_eq!(error.code.as_deref(), Some("missing_api_key"));
    assert!(error.message.contains("AI_GATEWAY_API_KEY"));
}

#[test]
fn model_id_keeps_the_vendor_segment() {
    let normalized = normalize_provider_model("vercel/openai/gpt-5.4").unwrap();
    assert_eq!(normalized.provider, "vercel");
    assert_eq!(normalized.model, "openai/gpt-5.4");
    assert_eq!(normalized.protocol, None);
    let anthropic = normalize_provider_model("vercel/anthropic/claude-sonnet-5").unwrap();
    assert_eq!(anthropic.model, "anthropic/claude-sonnet-5");
    let explicit = normalize_provider_model("vercel/chat/openai/gpt-5.4").unwrap();
    assert_eq!(explicit.protocol, Some(crate::protocol::Protocol::Chat));
    assert_eq!(explicit.model, "openai/gpt-5.4");
}

#[test]
fn adapter_reports_vercel_and_the_full_model_id() {
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: "vercel/openai/gpt-5.4".into(),
        api_key: Some("k".into()),
        api_key_env: None,
        api_url: None,
        max_tokens: None,
    })
    .expect("adapter");
    assert_eq!(adapter.provider_name(), "vercel");
    assert_eq!(adapter.model_name(), "openai/gpt-5.4");
}

#[test]
fn request_body_uses_the_reasoning_object() {
    let adapter = make_adapter("http://localhost");
    let options = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &options)
        .expect("body");
    assert_eq!(body["model"], "openai/gpt-5.4");
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["reasoning"]["exclude"], true);
    assert!(body.get("reasoning_effort").is_none());
}

#[tokio::test]
async fn streams_reasoning_and_sends_bearer_auth() {
    let (api_url, capture_rx) = serve_sse_once_capture(
        r#"data: {"choices":[{"delta":{"reasoning":"thinking"}}]}

data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;
    let adapter = make_adapter(&api_url);
    let options = RequestOptions {
        thinking: ThinkingLevel::Medium,
        ..Default::default()
    };
    let response = adapter
        .complete(&[], &[], &options, None)
        .await
        .expect("completes");
    assert_eq!(response.content.len(), 2);
    let raw = capture_rx.await.expect("request").to_lowercase();
    assert!(raw.contains("authorization: bearer test-key"));
}

/// Live smoke test: `AI_GATEWAY_API_KEY=… cargo test -p orchest-provider-http
/// vercel_live -- --ignored`.
#[tokio::test]
#[ignore = "needs AI_GATEWAY_API_KEY and network"]
async fn vercel_live_smoke() {
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: std::env::var("VERCEL_SMOKE_MODEL")
            .unwrap_or_else(|_| "vercel/openai/gpt-5.4-mini".into()),
        api_key: None,
        api_key_env: None,
        api_url: None,
        max_tokens: Some(32),
    })
    .expect("adapter");
    let message = crate::Message {
        role: crate::Role::User,
        content: vec![crate::ContentBlock::Text(
            "Reply with the single word: pong".into(),
        )],
    };
    let response = adapter
        .complete(&[message], &[], &RequestOptions::default(), None)
        .await
        .expect("live completion");
    assert!(!response.content.is_empty());
}
