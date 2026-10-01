use tokio::sync::mpsc;

use super::*;
use crate::chat::ChatAdapter;
use crate::providers::anthropic::test_util::*;
use crate::test_env::{lock_env, lock_env_async, EnvVarGuard};
use crate::{
    CapabilitySource, ContentBlock, Message, RequestOptions, Role, StopReason, StreamEvent,
    ThinkingLevel,
};

fn default_options() -> RequestOptions {
    RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    }
}

fn make_adapter(api_url: &str) -> ChatAdapter {
    ChatAdapter::for_test("openrouter", "anthropic/claude-sonnet-4", api_url, 4096)
}

#[test]
fn default_api_url() {
    assert_eq!(
        super::resolve_url(None).unwrap(),
        "https://openrouter.ai/api/v1/chat/completions"
    );
}

#[test]
fn config_api_key_takes_precedence() {
    use orchest_provider_core::registry::ProviderConfig;
    let config =
        ProviderConfig::new("openrouter", "anthropic/claude-sonnet-4").with_api_key("explicit-key");
    assert!(super::resolve_api_key(&config).is_ok());
}

#[test]
fn empty_api_key_still_resolves() {
    use orchest_provider_core::registry::ProviderConfig;
    // Empty string is still Some (non-empty validation isn't done here).
    let config = ProviderConfig::new("openrouter", "anthropic/claude-sonnet-4").with_api_key("");
    assert!(super::resolve_api_key(&config).is_ok());
}

#[tokio::test]
async fn sends_custom_headers() {
    // The routing headers come from env-declared HeaderValue::Env on the entry.
    let _env_lock = lock_env_async().await;
    let _title = EnvVarGuard::set("OPENROUTER_APP_TITLE", "TestApp");
    let _site = EnvVarGuard::set("OPENROUTER_SITE_URL", "https://example.com");

    let (api_url, capture_rx) = serve_sse_once_capture(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let _ = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should succeed");

    let raw_request = capture_rx.await.expect("should capture request");
    let raw_lower = raw_request.to_lowercase();
    assert!(
        raw_lower.contains("x-openrouter-title: testapp"),
        "should contain X-OpenRouter-Title header, got: {raw_request}"
    );
    assert!(
        raw_lower.contains("http-referer: https://example.com"),
        "should contain HTTP-Referer header, got: {raw_request}"
    );
    assert!(
        raw_lower.contains("authorization: bearer test-key"),
        "should contain Authorization header"
    );
}

#[test]
fn model_passthrough() {
    let adapter = make_adapter("http://localhost");
    assert_eq!(adapter.model_name(), "anthropic/claude-sonnet-4");

    let (body, _) = adapter
        .request_body_for_test(&[], &[], &default_options())
        .expect("body");
    assert_eq!(body["model"], "anthropic/claude-sonnet-4");
}

#[tokio::test]
async fn reasoning_maps_to_thinking() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"reasoning":"let me think about this"}}]}

data: {"choices":[{"delta":{"content":"the answer"}}]}

data: {"choices":[{"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let (tx, mut rx) = mpsc::channel(32);
    let response = adapter
        .complete(&[], &[], &default_options(), Some(tx))
        .await
        .expect("should parse");
    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(matches!(events[0], StreamEvent::ThinkingStart));
    assert!(
        matches!(&events[1], StreamEvent::Thinking { delta } if delta == "let me think about this")
    );
    assert!(matches!(events[2], StreamEvent::ThinkingEnd { .. }));
    assert!(matches!(&events[3], StreamEvent::Text { delta } if delta == "the answer"));

    assert_eq!(response.content.len(), 2);
    assert!(
        matches!(&response.content[0], ContentBlock::Thinking { text, .. } if text.as_deref() == Some("let me think about this"))
    );
}

#[tokio::test]
async fn reasoning_details_preserved() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"reasoning":"thinking..."}}]}

data: {"choices":[{"delta":{"content":"done","reasoning_details":[{"type":"text","text":"step 1"},{"type":"text","text":"step 2"}]}}]}

data: {"choices":[{"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let (tx, mut rx) = mpsc::channel(32);
    let response = adapter
        .complete(&[], &[], &default_options(), Some(tx))
        .await
        .expect("should parse");
    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    // The ThinkingEnd event should contain the reasoning_details
    let thinking_end = events
        .iter()
        .find(|e| matches!(e, StreamEvent::ThinkingEnd { .. }))
        .expect("should have ThinkingEnd");
    if let StreamEvent::ThinkingEnd {
        provider_details, ..
    } = thinking_end
    {
        let details = provider_details.as_ref().expect("should have details");
        let arr = details.as_array().expect("should be array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "step 1");
        assert_eq!(arr[1]["type"], "text");
        assert_eq!(arr[1]["text"], "step 2");
    }

    // Content should also preserve details
    let thinking_block = response
        .content
        .iter()
        .find(|c| matches!(c, ContentBlock::Thinking { .. }))
        .expect("should have Thinking block");
    if let ContentBlock::Thinking {
        provider_details, ..
    } = thinking_block
    {
        let details = provider_details.as_ref().expect("should have details");
        let arr = details.as_array().expect("should be array");
        assert_eq!(arr.len(), 2);
    }
}

#[test]
fn reasoning_object_from_thinking_level() {
    let adapter = make_adapter("http://localhost");

    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning"]["effort"], "high");
    assert!(body["reasoning"].get("max_tokens").is_none());

    let opts = RequestOptions {
        thinking: ThinkingLevel::Max,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning"]["effort"], "max");

    let opts = RequestOptions {
        thinking: ThinkingLevel::Minimal,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning"]["effort"], "minimal");
}

#[test]
fn reasoning_effort_and_max_tokens_are_exclusive() {
    let adapter = make_adapter("http://localhost");

    // With budget_tokens → only max_tokens, no effort
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: Some(10000),
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning"]["max_tokens"], 10000);
    assert!(body["reasoning"].get("effort").is_none());

    // Without budget_tokens → only effort, no max_tokens
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: None,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning"]["effort"], "high");
    assert!(body["reasoning"].get("max_tokens").is_none());
}

#[test]
fn include_thinking_false_sends_exclude() {
    let adapter = make_adapter("http://localhost");

    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning"]["exclude"], true);

    // include_thinking: true → no exclude field
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: true,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert!(body["reasoning"].get("exclude").is_none());
}

#[test]
fn replays_multiple_reasoning_detail_blocks_without_reasoning_blocks() {
    use serde_json::json;
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Thinking {
                text: None,
                signature: None,
                provider_details: Some(json!([
                    {"type": "reasoning.text", "text": "first"},
                    {"type": "reasoning.signature", "signature": "sig1"}
                ])),
            },
            ContentBlock::Thinking {
                text: Some("fallback plaintext should not be sent".into()),
                signature: None,
                provider_details: Some(json!({"type": "reasoning.text", "text": "second"})),
            },
            ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "lookup".into(),
                input: json!({"q": "orchest"}),
            },
        ],
    }];

    let (body, _) = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect("body");
    let msg = &body["messages"][0];

    assert!(msg.get("reasoning_blocks").is_none());
    assert!(msg.get("reasoning").is_none());
    assert_eq!(
        msg["reasoning_details"],
        json!([
            {"type": "reasoning.text", "text": "first"},
            {"type": "reasoning.signature", "signature": "sig1"},
            {"type": "reasoning.text", "text": "second"}
        ])
    );
}

#[test]
fn invalid_reasoning_replay_details_return_error() {
    use serde_json::json;
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Thinking {
                text: None,
                signature: None,
                provider_details: Some(json!("not valid replay metadata")),
            },
            ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "lookup".into(),
                input: json!({"q": "orchest"}),
            },
        ],
    }];

    let err = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect_err("invalid replay metadata should fail");

    assert_eq!(err.code.as_deref(), Some("invalid_reasoning_replay"));
}

#[test]
fn provider_name_and_capabilities() {
    let adapter = make_adapter("http://localhost");
    assert_eq!(adapter.provider_name(), "openrouter");
    let caps = adapter.capabilities();
    assert_eq!(caps.source, CapabilitySource::Assumed);
    assert!(caps.reasoning.budget_tokens);
    assert!(caps.reasoning.output_exclusion);
}

#[tokio::test]
async fn stop_reason_end_turn() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");
    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

#[test]
fn openrouter_downgrades_minimax_only_roles_with_adjustment() {
    let adapter = make_adapter("http://localhost");
    let opts = default_options();
    for (role, expected_api_role) in [
        (Role::UserSystem, "system"),
        (Role::Group, "user"),
        (Role::SampleMessageUser, "user"),
        (Role::SampleMessageAi, "user"),
    ] {
        let messages = vec![Message {
            role,
            content: vec![ContentBlock::Text("hi".into())],
        }];
        let (body, adjustments) = adapter
            .request_body_for_test(&messages, &[], &opts)
            .expect("valid request");
        assert_eq!(body["messages"][0]["role"], expected_api_role, "{role:?}");
        assert!(
            adjustments
                .iter()
                .any(|a| a.option == "role" && a.reason == "minimax_only_role_unsupported"),
            "{role:?} should record role adjustment"
        );
    }
}

// ---------------------------------------------------------------------------
// ADR-0002 slice 004: OpenRouter on ChatProtocolFactory + OpenRouterProfile,
// interpret_usage hook, and HeaderValue::Env resolution.
// ---------------------------------------------------------------------------

use crate::protocol::{
    provider_entry, resolve_headers, HeaderValue, Protocol, ProviderProfile, ResolvedModel,
};
use crate::TokenUsage;

fn cx() -> ResolvedModel<'static> {
    ResolvedModel {
        provider: provider_entry("openrouter").unwrap(),
        protocol: Protocol::Chat,
        model: "anthropic/claude-sonnet-4",
        catalog: None,
    }
}

#[test]
fn interpret_usage_reports_missing_usage() {
    let mut usage = TokenUsage::default();
    let adjustments =
        OPENROUTER_PROFILE.interpret_usage(&cx(), &serde_json::Value::Null, &mut usage);
    assert!(adjustments
        .iter()
        .any(|a| a.option == "usage" && a.reason == "usage_not_reported"));
}

#[test]
fn interpret_usage_passes_present_usage() {
    let mut usage = TokenUsage {
        input_tokens: 10,
        output_tokens: 5,
        ..Default::default()
    };
    let adjustments =
        OPENROUTER_PROFILE.interpret_usage(&cx(), &serde_json::Value::Null, &mut usage);
    assert!(adjustments.is_empty());
}

#[test]
fn entry_declares_env_routing_headers() {
    let entry = provider_entry("openrouter").expect("openrouter migrated");
    let names: Vec<_> = entry.extra_headers.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"X-OpenRouter-Title"));
    assert!(names.contains(&"HTTP-Referer"));
    assert!(entry
        .extra_headers
        .iter()
        .all(|(_, v)| matches!(v, HeaderValue::Env(_))));
}

#[test]
fn resolve_headers_reads_env_values() {
    let _env_lock = lock_env();
    let _title = EnvVarGuard::set("OPENROUTER_APP_TITLE", "MyApp");
    let _site = EnvVarGuard::remove("OPENROUTER_SITE_URL");

    let entry = provider_entry("openrouter").unwrap();
    let headers = resolve_headers(entry);
    // Set var resolves; unset var is skipped.
    assert_eq!(
        headers
            .iter()
            .find(|(n, _)| *n == "X-OpenRouter-Title")
            .map(|(_, v)| v.as_str()),
        Some("MyApp")
    );
    assert!(headers.iter().all(|(n, _)| *n != "HTTP-Referer"));
}

#[tokio::test]
async fn create_adapter_from_config_routes_openrouter_multi_segment() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;

    // Multi-segment model id: the second segment ("anthropic") stays part of the
    // model name, not treated as a protocol.
    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "openrouter/anthropic/claude-opus-4-8".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: Some(128),
    })
    .expect("openrouter multi-segment resolves through the protocol-factory path");

    assert_eq!(adapter.provider_name(), "openrouter");
    assert_eq!(adapter.model_name(), "anthropic/claude-opus-4-8");

    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("request should complete");
    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "hi"));
}
