use tokio::sync::mpsc;

use super::test_util::*;
use super::{resolve_url, ANTHROPIC_PROFILE};
use crate::messages::response::map_stop_reason;
use crate::messages::MessagesAdapter;
use crate::protocol::{Protocol, ProviderProfile, ResolvedModel};
use crate::ModelAdapter;
use crate::{
    defaults, CachePolicy, CapabilitySource, ContentBlock, MediaSource, Message, RequestOptions,
    Role, StopReason, StreamEvent, ThinkingLevel,
};

const MINIMAL_SSE: &str = r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#;

fn default_options() -> RequestOptions {
    RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    }
}

/// The shared Messages core carrying Anthropic's entry + profile. `api_url` is the
/// already-resolved endpoint (URL resolution is covered separately).
fn make_adapter(api_url: &str) -> MessagesAdapter {
    MessagesAdapter::for_test("anthropic", "claude-test", api_url, 128)
}

fn adapter_with(model: &str, max_tokens: u32) -> MessagesAdapter {
    MessagesAdapter::for_test("anthropic", model, "http://localhost", max_tokens)
}

/// Whether Anthropic's profile treats `model` as adaptive-thinking, via a
/// throwaway resolution context (the shared core's gate).
fn supports_adaptive(model: &str) -> bool {
    let entry = crate::protocol::provider_entry("anthropic").expect("anthropic entry");
    let cx = ResolvedModel {
        provider: entry,
        protocol: Protocol::Messages,
        model,
        catalog: crate::catalog::find_model(model),
    };
    ANTHROPIC_PROFILE.messages_supports_adaptive(&cx)
}

#[test]
fn uses_default_api_url() {
    assert_eq!(
        resolve_url(None).expect("default url resolves"),
        defaults::anthropic::API_URL
    );
}

#[test]
fn uses_custom_api_url() {
    let api_url = "https://compatible.example.com/v1/messages";
    assert_eq!(
        resolve_url(Some(api_url)).expect("custom url resolves"),
        api_url
    );
}

#[test]
fn appends_messages_endpoint() {
    assert_eq!(
        resolve_url(Some("https://openrouter.ai/api")).expect("url resolves"),
        "https://openrouter.ai/api/v1/messages"
    );
}

#[test]
fn rejects_empty_api_url() {
    let error = resolve_url(Some(" ")).expect_err("empty api url should be rejected");
    assert_eq!(error.code.as_deref(), Some("invalid_api_url"));
}

#[tokio::test]
async fn stream_thinking_boundaries() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":3}}}

event: content_block_start
data: {"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"first "}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"second"}}

event: content_block_stop
data: {"content_block":{"signature":"sig-abc-123"}}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

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

    assert!(matches!(events[0], StreamEvent::ThinkingStart));
    assert!(matches!(&events[1], StreamEvent::Thinking { delta } if delta == "first "));
    assert!(matches!(&events[2], StreamEvent::Thinking { delta } if delta == "second"));
    assert!(
        matches!(&events[3], StreamEvent::ThinkingEnd { ref signature, .. } if signature.as_deref() == Some("sig-abc-123"))
    );
    assert!(matches!(events[4], StreamEvent::Done { .. }));
    assert_eq!(response.usage.input_tokens, 3);
    assert_eq!(response.usage.output_tokens, 5);
}

#[tokio::test]
async fn stream_rejects_malformed_sse() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":3}}}

event: content_block_delta
data: {NOT VALID JSON!!!}

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let err = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect_err("malformed JSON should produce an error");

    assert_eq!(err.code.as_deref(), Some("invalid_json"));
    assert!(err.message.contains("malformed SSE JSON"));
    assert!(err.upstream.is_some());
}

#[tokio::test]
async fn stream_thinking_to_content_block() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"reasoning here"}}

event: content_block_stop
data: {"content_block":{"signature":"my-sig"}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"answer"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.content.len(), 2);
    match &response.content[0] {
        ContentBlock::Thinking {
            text,
            signature,
            provider_details,
        } => {
            assert_eq!(text.as_deref(), Some("reasoning here"));
            assert_eq!(signature.as_deref(), Some("my-sig"));
            assert!(provider_details.is_none());
        }
        _ => panic!("expected Thinking block"),
    }
    assert!(matches!(&response.content[1], ContentBlock::Text(t) if t == "answer"));
}

#[test]
fn thinking_level_maps_to_budget() {
    let adapter = adapter_with("claude-3-opus", 4096); // old model, uses enabled mode

    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 32768);
}

#[test]
fn thinking_budget_override() {
    let adapter = adapter_with("claude-3-opus", 4096);

    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: Some(8000),
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["budget_tokens"], 8000);
}

#[test]
fn thinking_budget_ignored_in_adaptive_reports_adjustment() {
    let adapter = adapter_with("claude-sonnet-4-20250514", 4096);

    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        thinking_budget_tokens: Some(8000),
        ..Default::default()
    };
    let (body, adjustments) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["type"], "adaptive");
    assert_eq!(body["output_config"]["effort"], "high");
    assert!(body["thinking"].get("budget_tokens").is_none());
    assert_eq!(adjustments.len(), 1);
    assert_eq!(adjustments[0].reason, "unsupported_in_adaptive_thinking");
}

#[test]
fn adaptive_uses_output_config_effort() {
    let adapter = adapter_with("claude-opus-4-20250514", 4096);

    for (level, expected) in [
        (ThinkingLevel::Low, "low"),
        (ThinkingLevel::Medium, "medium"),
        (ThinkingLevel::High, "high"),
        (ThinkingLevel::XHigh, "xhigh"),
        (ThinkingLevel::Max, "max"),
    ] {
        let opts = RequestOptions {
            thinking: level,
            ..Default::default()
        };
        let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], expected, "level {level:?}");
    }
}

#[test]
fn include_thinking_false_maps_to_omitted() {
    let adapter = adapter_with("claude-sonnet-4-20250514", 4096);

    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["display"], "omitted");
}

#[test]
fn thinking_budget_lifts_max_tokens_for_default_options() {
    // Default options (thinking Medium → budget 10240) on a non-adaptive
    // model: the 4096 max_tokens default is below the budget, which Anthropic
    // rejects with a 400 — lift it and record the adjustment.
    let adapter = adapter_with("claude-3-opus", 4096);

    let (body, adjustments) = adapter.request_body_for_test(&[], &[], &RequestOptions::default());
    let budget = body["thinking"]["budget_tokens"]
        .as_u64()
        .expect("budget_tokens present");
    let max_tokens = body["max_tokens"].as_u64().expect("max_tokens present");
    assert!(
        max_tokens > budget,
        "wire must satisfy max_tokens > budget_tokens"
    );
    assert_eq!(max_tokens, 10240 + 4096);
    let adj = adjustments
        .iter()
        .find(|a| a.option == "max_tokens")
        .expect("max_tokens adjustment recorded");
    assert_eq!(adj.requested, serde_json::json!(4096));
    assert_eq!(adj.applied, serde_json::json!(10240 + 4096));
    assert_eq!(adj.reason, "max_tokens_below_thinking_budget");
}

#[test]
fn explicit_max_tokens_below_budget_lifted_and_recorded() {
    let adapter = adapter_with("claude-3-opus", 4096);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Low, // budget 4096
        max_tokens: Some(2048),
        ..Default::default()
    };
    let (body, adjustments) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["budget_tokens"], 4096);
    assert_eq!(body["max_tokens"], 4096 + 4096);
    let adj = adjustments
        .iter()
        .find(|a| a.option == "max_tokens")
        .expect("max_tokens adjustment recorded");
    assert_eq!(adj.requested, serde_json::json!(2048));
    assert_eq!(adj.applied, serde_json::json!(4096 + 4096));
}

#[test]
fn budget_below_max_tokens_is_not_adjusted() {
    let adapter = adapter_with("claude-3-opus", 8192);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Low, // budget 4096 < 8192
        ..Default::default()
    };
    let (body, adjustments) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["budget_tokens"], 4096);
    assert_eq!(body["max_tokens"], 8192, "legal combination untouched");
    assert!(
        adjustments.iter().all(|a| a.option != "max_tokens"),
        "no adjustment recorded"
    );
}

#[test]
fn adaptive_path_leaves_max_tokens_alone() {
    let adapter = adapter_with("claude-sonnet-4-20250514", 4096);

    let (body, adjustments) = adapter.request_body_for_test(&[], &[], &RequestOptions::default());
    assert_eq!(body["thinking"]["type"], "adaptive");
    assert_eq!(body["max_tokens"], 4096);
    assert!(adjustments.iter().all(|a| a.option != "max_tokens"));
}

#[test]
fn capabilities_read_context_window_from_profile() {
    // The profile's context_window flows into adapter capabilities — the
    // source the runtime backfills ModelSpec.context_window_size from (#221).
    let sonnet = adapter_with("claude-sonnet-4-20250514", 4096);
    assert_eq!(sonnet.capabilities().context_window_size, Some(1_000_000));
    let haiku = adapter_with("claude-haiku-4-5", 4096);
    assert_eq!(haiku.capabilities().context_window_size, Some(200_000));
}

/// Recursive wire-level assertion helper: true when `cache_control` appears
/// anywhere under `v`.
fn contains_cache_control(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Object(map) => {
            map.contains_key("cache_control") || map.values().any(contains_cache_control)
        }
        serde_json::Value::Array(arr) => arr.iter().any(contains_cache_control),
        _ => false,
    }
}

fn system_and_user_messages() -> Vec<Message> {
    vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text("be helpful".into())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hi".into())],
        },
    ]
}

/// The message sequence a multi-turn `AgentRun::start_with_messages` run
/// assembles — [System(prompt)] + history (incl. a ToolUse/ToolResult pair)
/// + [User(new input)] — must lower to a legal Anthropic Messages request:
/// non-empty top-level `system`, role boundaries preserved, tool pairing
/// intact.
#[test]
fn multi_turn_history_with_tool_blocks_is_wire_legal() {
    let adapter = adapter_with("claude-test", 128);
    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::None,
        ..Default::default()
    };
    let messages = vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text("you are a lyricist".into())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("draft a chorus".into())],
        },
        Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text("checking the theme".into()),
                ContentBlock::ToolUse {
                    id: "toolu_1".into(),
                    name: "theme_lookup".into(),
                    input: serde_json::json!({"song": "rainy night"}),
                },
            ],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_1".into(),
                content: serde_json::json!("rain"),
            }],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("now the verse".into())],
        },
    ];

    let (body, _) = adapter.request_body_for_test(&messages, &[], &opts);

    assert_eq!(
        body["system"],
        serde_json::json!("you are a lyricist"),
        "system prompt reaches a non-empty top-level system field"
    );
    let wire_messages = body["messages"].as_array().expect("messages array");
    let roles: Vec<&str> = wire_messages
        .iter()
        .map(|m| m["role"].as_str().expect("role is a string"))
        .collect();
    assert_eq!(
        roles,
        vec!["user", "assistant", "user", "user"],
        "history role boundaries preserved; no inline system entries"
    );
    let assistant_blocks = wire_messages[1]["content"]
        .as_array()
        .expect("assistant content blocks");
    assert!(assistant_blocks
        .iter()
        .any(|b| b["type"] == "tool_use" && b["id"] == "toolu_1"));
    let result_blocks = wire_messages[2]["content"]
        .as_array()
        .expect("tool result content blocks");
    assert!(result_blocks
        .iter()
        .any(|b| b["type"] == "tool_result" && b["tool_use_id"] == "toolu_1"));
}

#[test]
fn cache_policy_auto_breakpoint_on_last_system_block() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::Auto,
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&system_and_user_messages(), &[], &opts);
    assert!(
        body.get("cache_control").is_none(),
        "no top-level cache_control"
    );
    let system = body["system"]
        .as_array()
        .expect("system switches to block-array form");
    let last = system.last().expect("at least one system block");
    assert_eq!(last["type"], "text");
    assert_eq!(last["text"], "be helpful");
    assert_eq!(
        last["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
    assert!(
        !contains_cache_control(&body["messages"]),
        "single breakpoint: messages stay clean"
    );
}

#[test]
fn cache_policy_auto_breakpoint_on_last_message_block_without_system() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::Auto,
        ..Default::default()
    };
    let messages = vec![
        Message {
            role: Role::User,
            content: vec![
                ContentBlock::Text("first".into()),
                ContentBlock::Text("second".into()),
            ],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("last".into())],
        },
    ];
    let (body, _) = adapter.request_body_for_test(&messages, &[], &opts);
    assert!(
        body.get("cache_control").is_none(),
        "no top-level cache_control"
    );
    assert!(body.get("system").is_none());
    let msgs = body["messages"].as_array().expect("messages array");
    assert!(
        !contains_cache_control(&msgs[0]),
        "earlier messages carry no breakpoint"
    );
    let blocks = msgs[1]["content"].as_array().expect("content blocks");
    assert_eq!(
        blocks.last().expect("last block")["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
}

#[test]
fn cache_policy_long_sets_1h_ttl_on_block() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::Long,
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&system_and_user_messages(), &[], &opts);
    assert!(body.get("cache_control").is_none());
    let system = body["system"].as_array().expect("system block array");
    assert_eq!(
        system.last().expect("last system block")["cache_control"],
        serde_json::json!({"type": "ephemeral", "ttl": "1h"})
    );
}

#[test]
fn cache_policy_none_emits_no_cache_control_anywhere() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::None,
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&system_and_user_messages(), &[], &opts);
    assert!(
        !contains_cache_control(&body),
        "no cache_control anywhere on the wire body"
    );
    assert!(
        body["system"].is_string(),
        "system stays a plain string without caching"
    );
}

#[test]
fn thinking_budget_equal_to_max_tokens_is_lifted() {
    // Anthropic requires strictly max_tokens > budget_tokens: the equality
    // path (ThinkingLevel::Max pins budget = effective max_tokens) must also
    // lift, not 400.
    let adapter = adapter_with("claude-3-opus", 8192);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Max,
        ..Default::default()
    };
    let (body, adjustments) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["thinking"]["budget_tokens"], 8192);
    assert_eq!(body["max_tokens"], 8192 + 4096);
    assert!(
        adjustments.iter().any(|a| a.option == "max_tokens"),
        "equality path records the lift"
    );
}

#[test]
fn cache_policy_auto_no_system_empty_messages_emits_no_breakpoint() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::Auto,
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
    assert!(
        !contains_cache_control(&body),
        "nothing to attach to: no cache_control anywhere"
    );
}

#[test]
fn cache_policy_auto_skips_thinking_block_for_cacheable_one() {
    // Thinking blocks carry no `cache_control` in the API schema: when the
    // last block is a thinking block, the breakpoint must fall back to the
    // previous cacheable block instead of tagging the thinking block.
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        cache_policy: CachePolicy::Auto,
        ..Default::default()
    };
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text("answer".into()),
            ContentBlock::Thinking {
                text: Some("hmm".into()),
                signature: None,
                provider_details: None,
            },
        ],
    }];
    let (body, _) = adapter.request_body_for_test(&messages, &[], &opts);
    let msgs = body["messages"].as_array().expect("messages array");
    let blocks = msgs[0]["content"].as_array().expect("content blocks");
    let text_block = blocks
        .iter()
        .find(|b| b["type"] == "text")
        .expect("text block");
    assert_eq!(
        text_block["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
    let thinking_block = blocks
        .iter()
        .find(|b| b["type"] == "thinking")
        .expect("thinking block");
    assert!(
        thinking_block.get("cache_control").is_none(),
        "thinking blocks carry no cache_control"
    );
}

#[tokio::test]
async fn cache_usage_mapped_to_token_usage() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":100,"cache_read_input_tokens":50,"cache_creation_input_tokens":20}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}

event: message_stop
data: {}

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.usage.input_tokens, 100);
    assert_eq!(response.usage.cache_read_tokens, 50);
    assert_eq!(response.usage.cache_write_tokens, 20);
}

#[test]
fn temperature_forwarded() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        temperature: Some(0.7),
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
    assert!(
        body["temperature"].as_f64().unwrap() > 0.69
            && body["temperature"].as_f64().unwrap() < 0.71
    );
}

#[test]
fn adaptive_vs_enabled_mode() {
    assert!(supports_adaptive("claude-sonnet-4-20250514"));
    assert!(!supports_adaptive("claude-3-opus"));
}

#[tokio::test]
async fn tool_use_start_and_end_emitted() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"tool_1","name":"read_file"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"/tmp\"}"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}

event: message_stop
data: {}

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

    assert!(
        matches!(&events[0], StreamEvent::ToolUseStart { id, name } if id == "tool_1" && name == "read_file")
    );
    assert!(matches!(&events[1], StreamEvent::ToolUseArgsChunk { id, .. } if id == "tool_1"));
    assert!(matches!(&events[2], StreamEvent::ToolUseEnd { id } if id == "tool_1"));
    assert!(matches!(events[3], StreamEvent::Done { .. }));
    assert_eq!(response.stop_reason, StopReason::ToolUse);
}

#[tokio::test]
async fn parallel_tool_uses_end_each() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"t1","name":"a"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{}"}}

event: content_block_stop
data: {}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"t2","name":"b"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{}"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}

event: message_stop
data: {}

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let (tx, mut rx) = mpsc::channel(32);
    adapter
        .complete(&[], &[], &default_options(), Some(tx))
        .await
        .expect("should parse");

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    let end_ids: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ToolUseEnd { id } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(end_ids, vec!["t1", "t2"]);
}

#[tokio::test]
async fn tx_none_skips_events() {
    let api_url = serve_sse_once(MINIMAL_SSE).await;
    let adapter = make_adapter(&api_url);

    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should parse");

    assert_eq!(response.content.len(), 1);
    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "Hello"));
    assert_eq!(response.usage.input_tokens, 10);
    assert_eq!(response.usage.output_tokens, 5);
    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

#[test]
fn max_tokens_override() {
    let adapter = adapter_with("claude-test", 128);

    let opts = RequestOptions {
        thinking: ThinkingLevel::Off,
        max_tokens: Some(4096),
        ..Default::default()
    };
    let (body, _) = adapter.request_body_for_test(&[], &[], &opts);
    assert_eq!(body["max_tokens"], 4096);

    let opts_none = RequestOptions {
        thinking: ThinkingLevel::Off,
        max_tokens: None,
        ..Default::default()
    };
    let (body2, _) = adapter.request_body_for_test(&[], &[], &opts_none);
    assert_eq!(body2["max_tokens"], 128);
}

#[tokio::test]
async fn stream_interrupted_returns_error() {
    let api_url = serve_partial_sse(
        r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"partial"}}

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let err = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect_err("interrupted stream should error");

    assert_eq!(err.code.as_deref(), Some("stream_interrupted"));
}

#[tokio::test]
async fn missing_usage_reports_adjustment() {
    let api_url = serve_sse_once(
        r#"event: message_start
data: {"message":{"usage":{}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"}}

event: message_stop
data: {}

"#,
    )
    .await;

    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("should succeed");

    let adj = response
        .option_adjustments
        .iter()
        .find(|a| a.reason == "usage_not_reported");
    assert!(adj.is_some(), "should report usage_not_reported adjustment");
}

#[tokio::test]
async fn done_usage_matches_model_response() {
    let api_url = serve_sse_once(MINIMAL_SSE).await;
    let adapter = make_adapter(&api_url);

    let (tx, mut rx) = mpsc::channel(16);
    let response = adapter
        .complete(&[], &[], &default_options(), Some(tx))
        .await
        .expect("should parse");

    let mut done_usage = None;
    while let Some(e) = rx.recv().await {
        if let StreamEvent::Done { usage } = e {
            done_usage = Some(usage);
        }
    }

    let done_usage = done_usage.expect("should have Done event");
    assert_eq!(done_usage.input_tokens, response.usage.input_tokens);
    assert_eq!(done_usage.output_tokens, response.usage.output_tokens);
    assert!(
        response.usage.cost_usd.is_some(),
        "response should have cost_usd filled by adapter"
    );
}

#[test]
fn provider_name_and_model_name() {
    let adapter = adapter_with("claude-sonnet-4-20250514", 4096);
    assert_eq!(adapter.provider_name(), "anthropic");
    assert_eq!(adapter.model_name(), "claude-sonnet-4-20250514");
}

#[test]
fn capabilities_reports_static_source() {
    let adapter = adapter_with("claude-sonnet-4-20250514", 4096);

    let caps = adapter.capabilities();
    assert!(caps.streaming);
    assert!(caps.tool_use);
    assert!(caps.reasoning.supported);
    assert!(caps.prompt_cache.supported);
    assert_eq!(caps.source, CapabilitySource::Static);
}

#[tokio::test]
async fn stop_reason_mapping() {
    for (raw, expected) in [
        ("end_turn", StopReason::EndTurn),
        ("tool_use", StopReason::ToolUse),
        ("max_tokens", StopReason::MaxTokens),
        ("stop_sequence", StopReason::StopSequence),
        ("pause_turn", StopReason::Pause),
        ("compaction", StopReason::Pause),
        ("refusal", StopReason::Refusal),
        (
            "model_context_window_exceeded",
            StopReason::ContextWindowExceeded,
        ),
    ] {
        assert_eq!(map_stop_reason(raw), expected, "for {raw}");
    }
    assert!(
        matches!(map_stop_reason("unknown_reason"), StopReason::Other(s) if s == "unknown_reason")
    );
}

// ---------------------------------------------------------------------------
// v0.9.10 multimodal additions: real Image serialization + Video/Audio/
// MidConvSystem drop + Minimax-only role downgrade.
// ---------------------------------------------------------------------------

#[test]
fn anthropic_serializes_image_url() {
    let adapter = make_adapter("http://example.com/v1/messages");
    let opts = default_options();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Image {
            source: MediaSource::Url {
                url: "https://example.com/cat.png".into(),
            },
            detail: None,
        }],
    }];
    let (body, adjustments) = adapter.request_body_for_test(&messages, &[], &opts);
    assert!(adjustments.is_empty(), "image is natively supported");
    let blocks = &body["messages"][0]["content"];
    assert_eq!(blocks[0]["type"], "image");
    assert_eq!(blocks[0]["source"]["type"], "url");
    assert_eq!(blocks[0]["source"]["url"], "https://example.com/cat.png");
}

#[test]
fn anthropic_serializes_image_base64() {
    let adapter = make_adapter("http://example.com/v1/messages");
    let opts = default_options();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Image {
            source: MediaSource::Base64 {
                media_type: "image/png".into(),
                data: "iVBORw0KGgo=".into(),
            },
            detail: Some("high".into()),
        }],
    }];
    let (body, _) = adapter.request_body_for_test(&messages, &[], &opts);
    let source = &body["messages"][0]["content"][0]["source"];
    assert_eq!(source["type"], "base64");
    assert_eq!(source["media_type"], "image/png");
    assert_eq!(source["data"], "iVBORw0KGgo=");
}

#[test]
fn anthropic_drops_video_audio_mid_conv_system_with_adjustments() {
    let adapter = make_adapter("http://example.com/v1/messages");
    let opts = default_options();
    let messages = vec![Message {
        role: Role::User,
        content: vec![
            ContentBlock::Video {
                source: MediaSource::Url {
                    url: "https://example.com/v.mp4".into(),
                },
                fps: Some(24.0),
                detail: None,
                max_long_side_pixel: None,
            },
            ContentBlock::Audio {
                source: MediaSource::Url {
                    url: "https://example.com/a.mp3".into(),
                },
            },
            ContentBlock::MidConvSystem("reset persona".into()),
        ],
    }];
    let (body, adjustments) = adapter.request_body_for_test(&messages, &[], &opts);
    assert_eq!(body["messages"][0]["content"].as_array().unwrap().len(), 0);
    assert_eq!(adjustments.len(), 3);
    let reasons: Vec<_> = adjustments.iter().map(|a| a.reason.as_str()).collect();
    assert!(reasons
        .iter()
        .all(|r| *r == "anthropic_unsupported_content_block"));
    let requested: Vec<_> = adjustments
        .iter()
        .map(|a| a.requested.as_str().unwrap().to_string())
        .collect();
    assert_eq!(requested, vec!["video", "audio", "mid_conv_system"]);
}

#[test]
fn anthropic_downgrades_minimax_user_system_role() {
    let adapter = make_adapter("http://example.com/v1/messages");
    let opts = default_options();
    let messages = vec![Message {
        role: Role::UserSystem,
        content: vec![ContentBlock::Text("be a pirate".into())],
    }];
    let (body, adjustments) = adapter.request_body_for_test(&messages, &[], &opts);
    // The default (Anthropic) role mapping downgrades UserSystem to "user".
    assert_eq!(body["messages"][0]["role"], "user");
    let adj = adjustments
        .iter()
        .find(|a| a.option == "role")
        .expect("role adjustment recorded");
    assert_eq!(adj.requested, serde_json::json!("user_system"));
    assert_eq!(adj.applied, serde_json::json!("user"));
    assert_eq!(adj.reason, "minimax_only_role_dropped");
}

#[test]
fn anthropic_downgrades_minimax_group_and_sample_roles() {
    let adapter = make_adapter("http://example.com/v1/messages");
    let opts = default_options();
    for role in [Role::Group, Role::SampleMessageUser, Role::SampleMessageAi] {
        let messages = vec![Message {
            role,
            content: vec![ContentBlock::Text("hi".into())],
        }];
        let (body, adjustments) = adapter.request_body_for_test(&messages, &[], &opts);
        assert_eq!(
            body["messages"][0]["role"], "user",
            "downgrade for {role:?}"
        );
        assert!(
            adjustments
                .iter()
                .any(|a| a.option == "role" && a.reason == "minimax_only_role_dropped"),
            "adjustment for {role:?}"
        );
    }
}

#[test]
fn anthropic_forwards_service_tier() {
    // service_tier is a Messages-wire meta-option carried by the shared core;
    // Anthropic callers rarely set it, but it round-trips when present.
    let adapter = make_adapter("http://example.com/v1/messages");
    let opts = RequestOptions {
        service_tier: Some("priority".into()),
        ..default_options()
    };
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text("hi".into())],
    }];
    let (body, _) = adapter.request_body_for_test(&messages, &[], &opts);
    assert_eq!(body["service_tier"], "priority");
}

// ---------------------------------------------------------------------------
// ADR-0002: Anthropic on the MessagesProtocolFactory path.
// ---------------------------------------------------------------------------

#[test]
fn anthropic_migrated_to_messages_entry() {
    let entry = crate::protocol::provider_entry("anthropic")
        .expect("anthropic is migrated to the protocol path");
    assert_eq!(entry.name, "anthropic");
    assert_eq!(entry.protocols, &[Protocol::Messages]);
    // Post-collapse, Anthropic's capability/auth/encoding facts ride a profile.
    assert!(entry.profile_for(Protocol::Messages).is_some());
}

#[test]
fn messages_url_append_is_idempotent() {
    use super::normalize_messages_url;
    assert_eq!(
        normalize_messages_url("https://api.anthropic.com"),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(
        normalize_messages_url("https://api.anthropic.com/v1/messages"),
        "https://api.anthropic.com/v1/messages"
    );
}

#[tokio::test]
async fn create_adapter_from_config_routes_anthropic_through_new_path() {
    let api_url = serve_sse_once(MINIMAL_SSE).await;

    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "anthropic/claude-sonnet-5".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: Some(128),
    })
    .expect("anthropic resolves through the MessagesProtocolFactory path");

    assert_eq!(adapter.provider_name(), "anthropic");
    assert_eq!(adapter.model_name(), "claude-sonnet-5");

    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("request should complete");
    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "Hello"));
}
