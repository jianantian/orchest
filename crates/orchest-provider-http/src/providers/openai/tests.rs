use tokio::sync::mpsc;

use super::*;
use crate::chat::ChatAdapter;
use crate::providers::anthropic::test_util::*;
use crate::{
    CachePolicy, CompatibilityPolicy, ContentBlock, MediaSource, Message, RequestOptions,
    ResponseFormat, Role, StopReason, StreamEvent, ThinkingLevel,
};
use serde_json::json;

fn default_options() -> RequestOptions {
    RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    }
}

fn make_adapter(api_url: &str) -> ChatAdapter {
    ChatAdapter::for_test("openai", "gpt-4o-mini", api_url, 128)
}

#[test]
fn json_object_response_format_lowers_to_chat_wire() {
    let adapter = make_adapter("http://localhost/v1/chat/completions");
    let options = RequestOptions {
        response_format: ResponseFormat::JsonObject,
        ..default_options()
    };

    let (body, _) = adapter
        .request_body_for_test(&[], &[], &options)
        .expect("request body");

    assert_eq!(body["response_format"], json!({"type": "json_object"}));
}

#[test]
fn strips_prefix() {
    // The provider prefix is stripped by the parser, not the adapter.
    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "openai/gpt-4o-mini".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some("http://localhost/v1/chat/completions".into()),
        max_tokens: Some(128),
    })
    .expect("adapter");
    assert_eq!(adapter.model_name(), "gpt-4o-mini");
}

#[tokio::test]
async fn stream_text_and_tool_calls() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi "}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"{\"text\":"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hello\"}"}}]},"finish_reason":"tool_calls"}]}

data: {"usage":{"prompt_tokens":7,"completion_tokens":9}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let (tx, mut rx) = mpsc::channel(16);
    let response = adapter
        .complete(&[], &[], &default_options(), Some(tx))
        .await
        .expect("should parse");

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(matches!(&events[0], StreamEvent::Text { delta } if delta == "hi "));
    assert!(events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolUseStart { name, .. } if name == "echo")));
    assert!(events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolUseEnd { id } if id == "call_1")));
    assert_eq!(response.usage.input_tokens, 7);
    assert_eq!(response.usage.output_tokens, 9);
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert!(matches!(
        &response.content[1],
        ContentBlock::ToolUse { id, name, input }
            if id == "call_1" && name == "echo" && input["text"] == "hello"
    ));
}

#[test]
fn build_request_body_serializes_correctly() {
    let adapter = make_adapter("http://localhost/v1/chat/completions");

    let messages = vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text("You are helpful.".into())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        },
        Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text("Let me check.".into()),
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "get_weather".into(),
                    input: json!({"city": "NYC"}),
                },
            ],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: json!({"temp": 72}),
            }],
        },
    ];

    let (body, _) = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect("body");
    let api_msgs = body["messages"].as_array().unwrap();

    assert_eq!(api_msgs[0]["role"], "system");
    assert_eq!(api_msgs[0]["content"], "You are helpful.");
    assert_eq!(api_msgs[1]["role"], "user");
    assert_eq!(api_msgs[1]["content"], "Hello");
    assert_eq!(api_msgs[2]["role"], "assistant");
    assert_eq!(api_msgs[2]["content"], "Let me check.");
    let tc = api_msgs[2]["tool_calls"].as_array().unwrap();
    assert_eq!(tc[0]["id"], "call_1");
    assert_eq!(tc[0]["function"]["name"], "get_weather");
    assert_eq!(api_msgs[3]["role"], "tool");
    assert_eq!(api_msgs[3]["tool_call_id"], "call_1");
}

#[tokio::test]
async fn stream_rejects_invalid_tool_args() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"not valid json"}}]},"finish_reason":"tool_calls"}]}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let err = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect_err("should fail");
    assert_eq!(err.code.as_deref(), Some("invalid_tool_arguments"));
}

#[test]
fn thinking_level_maps_to_reasoning_effort() {
    let adapter = ChatAdapter::for_test("openai", "o3-mini", "http://localhost", 4096);
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["reasoning_effort"], "high");
}

#[test]
fn unsupported_reasoning_coerce_reports_adjustment() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        compatibility_policy: CompatibilityPolicy::Coerce,
        ..Default::default()
    };

    let (body, adjustments) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");

    assert!(body.get("reasoning_effort").is_none());
    assert!(adjustments.iter().any(|adjustment| {
        adjustment.option == "thinking" && adjustment.reason == "unsupported_reasoning_model"
    }));
}

#[tokio::test]
async fn unsupported_reasoning_strict_errors_before_request() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"should not be requested"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}

data: [DONE]

"#,
    )
    .await;
    let adapter = make_adapter(&api_url);
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        compatibility_policy: CompatibilityPolicy::Strict,
        ..Default::default()
    };

    let err = adapter
        .complete(&[], &[], &opts, None)
        .await
        .expect_err("unsupported reasoning should fail before request");

    assert_eq!(err.code.as_deref(), Some("unsupported_reasoning_model"));
}

#[test]
fn thinking_off_omits_reasoning_effort() {
    let adapter = ChatAdapter::for_test("openai", "o3-mini", "http://localhost", 4096);
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert!(body.get("reasoning_effort").is_none());
}

#[test]
fn cache_policy_long_reports_adjustment() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::Long,
        ..Default::default()
    };
    let (_, adjustments) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert!(adjustments
        .iter()
        .any(|a| a.reason == "unsupported_cache_retention"));
}

#[tokio::test]
async fn cache_tokens_reported() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":50},"completion_tokens_details":{"reasoning_tokens":10}}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.usage.cache_read_tokens, 50);
    assert_eq!(response.usage.cache_write_tokens, 0);
}

#[test]
fn temperature_forwarded() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        temperature: Some(0.5),
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert!(body["temperature"].as_f64().unwrap() > 0.49);
}

#[test]
fn max_tokens_override() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        max_tokens: Some(8192),
        ..Default::default()
    };
    let (body, _) = adapter
        .request_body_for_test(&[], &[], &opts)
        .expect("body");
    assert_eq!(body["max_tokens"], 8192);

    let opts_none = default_options();
    let (body2, _) = adapter
        .request_body_for_test(&[], &[], &opts_none)
        .expect("body");
    assert_eq!(body2["max_tokens"], 128);
}

#[tokio::test]
async fn reasoning_tokens_reported() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"completion_tokens_details":{"reasoning_tokens":20}}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.usage.reasoning_tokens, 20);
}

#[tokio::test]
async fn tx_none_skips_events() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}

data: [DONE]

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.content.len(), 1);
    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "hi"));
}

#[test]
fn provider_name_and_model_name() {
    let adapter = make_adapter("http://localhost");
    assert_eq!(adapter.provider_name(), "openai");
    assert_eq!(adapter.model_name(), "gpt-4o-mini");
}

#[test]
fn openai_downgrades_minimax_only_roles_with_adjustment() {
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
            .expect("body");
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
// C5: dropped content blocks must be visible (OptionAdjustment, never silent).
// ---------------------------------------------------------------------------

fn image_block() -> ContentBlock {
    ContentBlock::Image {
        source: MediaSource::Url {
            url: "https://example.com/x.png".into(),
        },
        detail: None,
    }
}

fn assert_content_block_drop(adjustments: &[crate::OptionAdjustment], kind: &str) {
    assert!(
        adjustments.iter().any(|a| a.option == "content_block"
            && a.requested == json!(kind)
            && a.applied == json!(null)
            && a.reason == "chat_unsupported_content_block"),
        "expected a content_block drop adjustment for {kind}: {adjustments:?}"
    );
}

#[test]
fn user_multimodal_block_drop_records_adjustment() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text("look".into()), image_block()],
    }];

    let (body, adjustments) = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect("body");

    let api_msgs = body["messages"].as_array().unwrap();
    assert_eq!(api_msgs[0]["role"], "user");
    assert_eq!(api_msgs[0]["content"], "look", "image is not on the wire");
    assert_content_block_drop(&adjustments, "image");
}

#[test]
fn system_non_text_block_drop_records_adjustment() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::System,
        content: vec![ContentBlock::Text("be helpful".into()), image_block()],
    }];

    let (body, adjustments) = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect("body");

    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "be helpful");
    assert_content_block_drop(&adjustments, "image");
}

#[test]
fn assistant_multimodal_block_drop_records_adjustment() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text("here".into()), image_block()],
    }];

    let (body, adjustments) = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect("body");

    assert_eq!(body["messages"][0]["role"], "assistant");
    assert_eq!(body["messages"][0]["content"], "here");
    assert_content_block_drop(&adjustments, "image");
}

#[test]
fn user_text_alongside_tool_result_drop_records_adjustment() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![
            ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: json!({"temp": 72}),
            },
            ContentBlock::Text("extra note".into()),
        ],
    }];

    let (body, adjustments) = adapter
        .request_body_for_test(&messages, &[], &default_options())
        .expect("body");

    let api_msgs = body["messages"].as_array().unwrap();
    assert_eq!(api_msgs.len(), 1, "only the wire tool message survives");
    assert_eq!(api_msgs[0]["role"], "tool");
    assert_eq!(api_msgs[0]["tool_call_id"], "call_1");
    assert_content_block_drop(&adjustments, "text");
}

// ---------------------------------------------------------------------------
// Entry / routing.
// ---------------------------------------------------------------------------

use crate::protocol::{provider_entry, Protocol};

#[test]
fn openai_migrated_to_protocol_entry() {
    let entry = provider_entry("openai").expect("openai is a protocol entry");
    assert_eq!(entry.name, "openai");
    assert_eq!(entry.protocols, &[Protocol::Chat]);
    assert_eq!(entry.default_api_key_env, "OPENAI_API_KEY");
    assert!(provider_entry("gemini").is_none());
}

#[tokio::test]
async fn create_adapter_from_config_routes_openai() {
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;

    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "openai/gpt-4.1".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: Some(128),
    })
    .expect("openai resolves through the protocol-factory path");

    assert_eq!(adapter.provider_name(), "openai");
    assert_eq!(adapter.model_name(), "gpt-4.1");

    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("request should complete");
    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "hi"));
    assert_eq!(response.usage.input_tokens, 3);
}

#[test]
fn chat_url_append_is_idempotent() {
    use super::request::normalize_chat_url;
    assert_eq!(
        normalize_chat_url("https://api.openai.com"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        normalize_chat_url("https://api.openai.com/v1"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        normalize_chat_url("https://api.openai.com/v1/chat/completions"),
        "https://api.openai.com/v1/chat/completions"
    );
}

// ---------------------------------------------------------------------------
// Capability facts: catalog canonical; name-prefix fallback for unlisted models.
// ---------------------------------------------------------------------------

fn adapter_for(model: &str) -> ChatAdapter {
    ChatAdapter::for_test("openai", model, "http://localhost", 128)
}

#[test]
fn capabilities_read_from_catalog_for_listed_model() {
    let caps = adapter_for("gpt-5.4").capabilities();
    assert!(caps.reasoning.supported);
    assert_eq!(caps.context_window_size, Some(1_000_000));
}

#[test]
fn unlisted_model_falls_back_to_prefix_tables() {
    let reasoning = adapter_for("o3-mini");
    assert!(
        reasoning.capabilities().reasoning.supported,
        "o3-mini reasoning via prefix fallback"
    );
    assert_eq!(
        reasoning.capabilities().context_window_size,
        Some(128_000),
        "unlisted context window via fallback default"
    );

    let non_reasoning = adapter_for("gpt-4o");
    assert!(!non_reasoning.capabilities().reasoning.supported);
}

#[tokio::test]
async fn strict_image_request_still_drops_without_error() {
    // Hotfix 2026-10-01 #319: the default `chat_image_input` keeps the C5 drop
    // semantics for non-DeepSeek Chat providers — no Strict rejection.
    let api_url = serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;
    let adapter = make_adapter(&api_url);
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text("hi".into()), image_block()],
    }];
    let opts = RequestOptions {
        thinking: crate::ThinkingLevel::Off,
        compatibility_policy: CompatibilityPolicy::Strict,
        ..Default::default()
    };
    let response = adapter
        .complete(&messages, &[], &opts, None)
        .await
        .expect("strict does not reject images for the default profile");
    assert_content_block_drop(&response.option_adjustments, "image");
}
