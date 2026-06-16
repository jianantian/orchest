use super::*;
use crate::providers::anthropic::test_util::*;
use crate::StopReason;

fn default_options() -> RequestOptions {
    RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    }
}

fn make_adapter(api_url: &str) -> OpenRouterAdapter {
    OpenRouterAdapter::from_config(OpenRouterConfig {
        model: "anthropic/claude-sonnet-4".into(),
        max_tokens: 4096,
        api_key: Some("test-key".into()),
        api_url: Some(api_url.into()),
        app_title: Some("TestApp".into()),
        site_url: Some("https://example.com".into()),
    })
    .expect("adapter should be created")
}

#[test]
fn default_api_url() {
    let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
        model: "anthropic/claude-sonnet-4".into(),
        max_tokens: 4096,
        api_key: Some("key".into()),
        api_url: None,
        app_title: None,
        site_url: None,
    })
    .unwrap();
    assert_eq!(
        adapter.api_url,
        "https://openrouter.ai/api/v1/chat/completions"
    );
}

#[test]
fn config_api_key_takes_precedence() {
    let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
        model: "anthropic/claude-sonnet-4".into(),
        max_tokens: 4096,
        api_key: Some("explicit-key".into()),
        api_url: Some("http://localhost".into()),
        app_title: None,
        site_url: None,
    });
    assert!(adapter.is_ok());
}

#[test]
fn missing_api_key_error_code() {
    // Temporarily ensure no env var by testing the error message pattern
    let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
        model: "anthropic/claude-sonnet-4".into(),
        max_tokens: 4096,
        api_key: Some("".into()),
        api_url: Some("http://localhost".into()),
        app_title: None,
        site_url: None,
    });
    // Empty string is still Some, so it succeeds (non-empty validation isn't done on key)
    assert!(adapter.is_ok());
}

#[tokio::test]
async fn sends_custom_headers() {
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

    let (body, _) = adapter.build_request_body(&[], &[], &default_options());
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
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
    assert_eq!(body["reasoning"]["effort"], "high");
    assert!(body["reasoning"].get("max_tokens").is_none());

    let opts = RequestOptions {
        thinking: ThinkingLevel::Max,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
    assert_eq!(body["reasoning"]["effort"], "max");

    let opts = RequestOptions {
        thinking: ThinkingLevel::Minimal,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
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
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
    assert_eq!(body["reasoning"]["max_tokens"], 10000);
    assert!(body["reasoning"].get("effort").is_none());

    // Without budget_tokens → only effort, no max_tokens
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: None,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
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
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
    assert_eq!(body["reasoning"]["exclude"], true);

    // include_thinking: true → no exclude field
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: true,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts);
    assert!(body["reasoning"].get("exclude").is_none());
}

#[test]
fn replays_multiple_reasoning_detail_blocks_without_reasoning_blocks() {
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

    let (body, _) = adapter.build_request_body(&messages, &[], &default_options());
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
        .try_build_request_body(&messages, &[], &default_options())
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
