use orchest_provider_core::registry::ProviderConfig;

use super::*;
use crate::chat::ChatAdapter;
use crate::providers::anthropic::test_util::*;
use crate::test_env::{lock_env, EnvVarGuard};
use crate::{
    create_adapter_from_config, normalize_provider_model, ProviderRuntimeConfig, RequestOptions,
    ThinkingLevel,
};

const MODEL: &str = "accounts/fireworks/models/kimi-k2-instruct";

fn make_adapter(api_url: &str) -> ChatAdapter {
    ChatAdapter::for_test("fireworks", MODEL, api_url, 4096)
}

fn runtime(model: &str, api_key: Option<&str>) -> ProviderRuntimeConfig {
    ProviderRuntimeConfig {
        model: model.into(),
        api_key: api_key.map(Into::into),
        api_key_env: None,
        api_url: None,
        max_tokens: None,
    }
}

#[test]
fn default_api_url() {
    assert_eq!(
        resolve_url(None).unwrap(),
        "https://api.fireworks.ai/inference/v1/chat/completions"
    );
}

#[test]
fn api_url_is_normalized() {
    for (input, expected) in [
        (
            "https://api.fireworks.ai/inference/v1/",
            "https://api.fireworks.ai/inference/v1/chat/completions",
        ),
        (
            "https://api.fireworks.ai/inference/v1/chat/completions",
            "https://api.fireworks.ai/inference/v1/chat/completions",
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
    let _key = EnvVarGuard::set("FIREWORKS_API_KEY", "env-key");
    let explicit = ProviderConfig::new("fireworks", "kimi-k2-instruct").with_api_key("explicit");
    assert_eq!(resolve_api_key(&explicit).unwrap(), "explicit");
    let from_env = ProviderConfig::new("fireworks", "kimi-k2-instruct");
    assert_eq!(resolve_api_key(&from_env).unwrap(), "env-key");
}

#[test]
fn missing_api_key_is_reported() {
    let _lock = lock_env();
    let _key = EnvVarGuard::remove("FIREWORKS_API_KEY");
    let error = create_adapter_from_config(runtime("fireworks/kimi-k2-instruct", None))
        .err()
        .expect("missing key errors");
    assert_eq!(error.code.as_deref(), Some("missing_api_key"));
    assert!(error.message.contains("FIREWORKS_API_KEY"));
}

#[test]
fn short_names_map_to_fireworks_hosted_models() {
    assert_eq!(upstream_model("kimi-k2-instruct"), MODEL);
    assert_eq!(upstream_model(MODEL), MODEL);
    let id = format!("fireworks/{MODEL}");
    let normalized = normalize_provider_model(&id).unwrap();
    assert_eq!(normalized.provider, "fireworks");
    assert_eq!(normalized.model, MODEL);
    assert_eq!(normalized.protocol, None);
}

#[test]
fn adapter_reports_the_upstream_model_id() {
    let full = format!("fireworks/{MODEL}");
    for id in ["fireworks/kimi-k2-instruct", full.as_str()] {
        let adapter = create_adapter_from_config(runtime(id, Some("k"))).expect("adapter");
        assert_eq!(adapter.provider_name(), "fireworks");
        assert_eq!(adapter.model_name(), MODEL);
    }
}

#[test]
fn request_body_uses_reasoning_effort() {
    let adapter = make_adapter("http://localhost");
    let options = RequestOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &options)
        .expect("body");
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["reasoning_effort"], "high");
    assert!(body.get("reasoning").is_none());
}

#[tokio::test]
async fn streams_reasoning_content_and_sends_bearer_auth() {
    let (api_url, capture_rx) = serve_sse_once_capture(
        r#"data: {"choices":[{"delta":{"reasoning_content":"thinking"}}]}

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

/// Live smoke test: `FIREWORKS_API_KEY=… cargo test -p orchest-provider-http
/// fireworks_live -- --ignored`.
#[tokio::test]
#[ignore = "needs FIREWORKS_API_KEY and network"]
async fn fireworks_live_smoke() {
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        max_tokens: Some(32),
        ..runtime(
            &std::env::var("FIREWORKS_SMOKE_MODEL")
                .unwrap_or_else(|_| "fireworks/kimi-k2-instruct".into()),
            None,
        )
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
