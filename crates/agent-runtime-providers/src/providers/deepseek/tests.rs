use serde_json::json;
use tokio::sync::mpsc;

use super::*;
use crate::providers::anthropic::test_util::*;
use crate::{ContentBlock, Role};

fn default_options() -> RequestOptions {
    RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    }
}

fn make_adapter(api_url: &str) -> DeepSeekAdapter {
    DeepSeekAdapter::from_config(DeepSeekConfig {
        model: "deepseek-chat".into(),
        max_tokens: 4096,
        api_key: Some("test-key".into()),
        api_url: Some(api_url.into()),
    })
    .expect("adapter should be created")
}

#[test]
fn default_api_url() {
    let adapter = DeepSeekAdapter::from_config(DeepSeekConfig {
        model: "deepseek-chat".into(),
        max_tokens: 4096,
        api_key: Some("key".into()),
        api_url: None,
    })
    .unwrap();
    assert_eq!(
        adapter.api_url,
        "https://api.deepseek.com/v1/chat/completions"
    );
}

#[test]

fn env_var_resolution() {
    // Remove DEEPSEEK_API_KEY from the environment so the test is
    // deterministic regardless of ambient shell configuration.
    let saved = std::env::var("DEEPSEEK_API_KEY").ok();
    std::env::remove_var("DEEPSEEK_API_KEY");

    let result = DeepSeekAdapter::from_config(DeepSeekConfig {
        model: "deepseek-chat".into(),
        max_tokens: 4096,
        api_key: None,
        api_url: Some("http://localhost".into()),
    });
    // Without DEEPSEEK_API_KEY set, this should fail.
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code.as_deref(), Some("missing_api_key"));

    // Restore the env var if it was previously set.
    if let Some(val) = saved {
        std::env::set_var("DEEPSEEK_API_KEY", val);
    }
}
#[test]
fn thinking_off_disables_reasoning() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, false);
    assert_eq!(body["thinking"]["type"], "disabled");
    assert!(body.get("reasoning_effort").is_none());
}

#[test]
fn thinking_levels_map_to_high_and_max() {
    let adapter = make_adapter("http://localhost");

    // Minimal → high
    let opts = RequestOptions {
        thinking: ThinkingLevel::Minimal,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "high");

    // Low → high
    let opts = RequestOptions {
        thinking: ThinkingLevel::Low,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert_eq!(body["reasoning_effort"], "high");

    // Medium → high
    let opts = RequestOptions {
        thinking: ThinkingLevel::Medium,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert_eq!(body["reasoning_effort"], "high");

    // High → high
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert_eq!(body["reasoning_effort"], "high");

    // XHigh → max
    let opts = RequestOptions {
        thinking: ThinkingLevel::XHigh,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert_eq!(body["reasoning_effort"], "max");

    // Max → max
    let opts = RequestOptions {
        thinking: ThinkingLevel::Max,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert_eq!(body["reasoning_effort"], "max");
}

#[test]
fn thinking_is_top_level_not_extra_body() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    // thinking must be a top-level field
    assert!(body.get("thinking").is_some());
    assert_eq!(body["thinking"]["type"], "enabled");
    // there should be no extra_body wrapper
    assert!(body.get("extra_body").is_none());
}

#[test]
fn omits_sampling_when_thinking_enabled() {
    let adapter = make_adapter("http://localhost");

    // Thinking enabled → no temperature/top_p
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        temperature: Some(0.7),
        top_p: Some(0.9),
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, true);
    assert!(body.get("temperature").is_none());
    assert!(body.get("top_p").is_none());

    // Thinking disabled → temperature/top_p present
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        temperature: Some(0.7),
        top_p: Some(0.9),
        ..Default::default()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, false);
    assert!(body.get("temperature").is_some());
    assert!(body.get("top_p").is_some());
}

#[tokio::test]
async fn include_thinking_false_coerce_reports_adjustment() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        compatibility_policy: CompatibilityPolicy::Coerce,
        ..Default::default()
    };
    let response = adapter
        .complete(&[], &[], &opts, None)
        .await
        .expect("should succeed");

    assert!(response
        .option_adjustments
        .iter()
        .any(|a| a.reason == "output_exclusion_unsupported_disables_reasoning"));
}

#[tokio::test]
async fn include_thinking_false_strict_errors() {
    let adapter = make_adapter("http://localhost:1");
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        compatibility_policy: CompatibilityPolicy::Strict,
        ..Default::default()
    };
    let err = adapter
        .complete(&[], &[], &opts, None)
        .await
        .expect_err("should fail");
    assert_eq!(
        err.code.as_deref(),
        Some("unsupported_reasoning_output_exclusion")
    );
}

#[test]
fn replays_reasoning_for_tool_call_turns() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Thinking {
                text: Some("I should call the tool".into()),
                signature: None,
                provider_details: None,
            },
            ContentBlock::Text("Let me check.".into()),
            ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "get_info".into(),
                input: json!({"q": "test"}),
            },
        ],
    }];

    let opts = default_options();
    let (body, _) = adapter.build_request_body(&messages, &[], &opts, false);
    let msg = &body["messages"][0];
    assert_eq!(msg["reasoning_content"], "I should call the tool");
    assert!(msg["tool_calls"].as_array().unwrap().len() == 1);
}

#[test]
fn omits_reasoning_for_non_tool_call_turns() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Thinking {
                text: Some("Just thinking here".into()),
                signature: None,
                provider_details: None,
            },
            ContentBlock::Text("Here's my answer.".into()),
        ],
    }];

    let opts = default_options();
    let (body, _) = adapter.build_request_body(&messages, &[], &opts, false);
    let msg = &body["messages"][0];
    assert!(msg.get("reasoning_content").is_none());
    assert_eq!(msg["content"], "Here's my answer.");
}

#[tokio::test]
async fn reasoning_maps_to_thinking() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"reasoning_content":"let me think"}}]}

data: {"choices":[{"delta":{"content":"answer"}}]}

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
    assert!(matches!(&events[1], StreamEvent::Thinking { delta } if delta == "let me think"));
    assert!(matches!(events[2], StreamEvent::ThinkingEnd { .. }));
    assert!(matches!(&events[3], StreamEvent::Text { delta } if delta == "answer"));

    assert_eq!(response.content.len(), 2);
    assert!(
        matches!(&response.content[0], ContentBlock::Thinking { text, .. } if text.as_deref() == Some("let me think"))
    );
    assert!(matches!(&response.content[1], ContentBlock::Text(t) if t == "answer"));
}

#[tokio::test]
async fn cache_hit_tokens_reported() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":40}}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.usage.cache_read_tokens, 40);
    assert_eq!(response.usage.cache_write_tokens, 0);
}

#[tokio::test]
async fn official_cache_hit_and_miss_tokens_reported() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":5,"prompt_cache_hit_tokens":37,"prompt_cache_miss_tokens":63}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.usage.cache_read_tokens, 37);
    assert_eq!(
        response.usage.details.get("prompt_cache_miss_tokens"),
        Some(&63)
    );
}

#[test]
fn provider_name_and_model_name() {
    let adapter = make_adapter("http://localhost");
    assert_eq!(adapter.provider_name(), "deepseek");
    assert_eq!(adapter.model_name(), "deepseek-chat");
}

#[test]
fn max_tokens_override() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        max_tokens: Some(8192),
        ..default_options()
    };
    let (body, _) = adapter.build_request_body(&[], &[], &opts, false);
    assert_eq!(body["max_tokens"], 8192);

    let (body2, _) = adapter.build_request_body(&[], &[], &default_options(), false);
    assert_eq!(body2["max_tokens"], 4096);
}
