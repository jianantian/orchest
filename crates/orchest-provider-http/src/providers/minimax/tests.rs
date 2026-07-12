use super::*;
use crate::providers::anthropic::test_util::*;
use crate::{ContentBlock, MediaSource, Message, RequestOptions, Role, StopReason, ThinkingLevel};

fn default_options() -> RequestOptions {
    RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    }
}

fn make_adapter(api_url: &str) -> MinimaxAdapter {
    MinimaxAdapter::from_config(MinimaxConfig {
        model: "MiniMax-M3".into(),
        max_tokens: 128,
        api_key: Some("key".into()),
        api_url: Some(api_url.into()),
    })
    .expect("adapter should be created")
}

#[test]
fn uses_default_api_url_when_not_provided() {
    let adapter = MinimaxAdapter::from_config(MinimaxConfig {
        model: "MiniMax-M3".into(),
        max_tokens: 128,
        api_key: Some("key".into()),
        api_url: None,
    })
    .expect("adapter should be created");
    assert_eq!(adapter.api_url, defaults::minimax::API_URL);
}

#[test]
fn normalize_messages_url_appends_anthropic_path_when_missing() {
    let adapter = make_adapter("https://api.minimaxi.com");
    assert_eq!(
        adapter.api_url,
        "https://api.minimaxi.com/anthropic/v1/messages"
    );
}

#[test]
fn provider_name_and_model_name() {
    let adapter = make_adapter("http://localhost");
    assert_eq!(adapter.provider_name(), "minimax");
    assert_eq!(adapter.model_name(), "MiniMax-M3");
}

#[test]
fn m3_supports_adaptive_m2_does_not() {
    let m3 = make_adapter("http://localhost");
    assert!(m3.supports_adaptive());
    let m2 = MinimaxAdapter::from_config(MinimaxConfig {
        model: "MiniMax-M2.7".into(),
        max_tokens: 128,
        api_key: Some("key".into()),
        api_url: Some("http://localhost".into()),
    })
    .unwrap();
    assert!(!m2.supports_adaptive());
}

#[test]
fn context_window_size_per_model() {
    let m3 = make_adapter("http://localhost");
    assert_eq!(m3.context_window_size(), 1_000_000);
    let m2 = MinimaxAdapter::from_config(MinimaxConfig {
        model: "MiniMax-M2.7-highspeed".into(),
        max_tokens: 128,
        api_key: Some("key".into()),
        api_url: Some("http://localhost".into()),
    })
    .unwrap();
    assert_eq!(m2.context_window_size(), 204_800);
}

#[test]
fn serializes_image_url_natively() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Image {
            source: MediaSource::Url {
                url: "https://example.com/cat.png".into(),
            },
            detail: None,
        }],
    }];
    let (body, adjustments) = adapter.build_request_body(&messages, &[], &default_options());
    assert!(adjustments.is_empty(), "image is natively supported");
    let block = &body["messages"][0]["content"][0];
    assert_eq!(block["type"], "image");
    assert_eq!(block["source"]["type"], "url");
    assert_eq!(block["source"]["url"], "https://example.com/cat.png");
}

#[test]
fn serializes_image_base64_with_detail() {
    let adapter = make_adapter("http://localhost");
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
    let (body, _) = adapter.build_request_body(&messages, &[], &default_options());
    let block = &body["messages"][0]["content"][0];
    assert_eq!(block["source"]["type"], "base64");
    assert_eq!(block["source"]["media_type"], "image/png");
    assert_eq!(block["detail"], "high");
}

#[test]
fn serializes_video_with_minimax_fields() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Video {
            source: MediaSource::Url {
                url: "https://example.com/v.mp4".into(),
            },
            fps: Some(8.0),
            detail: Some("low".into()),
            max_long_side_pixel: Some(720),
        }],
    }];
    let (body, adjustments) = adapter.build_request_body(&messages, &[], &default_options());
    assert!(adjustments.is_empty());
    let block = &body["messages"][0]["content"][0];
    assert_eq!(block["type"], "video");
    assert_eq!(block["fps"], 8.0);
    assert_eq!(block["max_long_side_pixel"], 720);
}

#[test]
fn serializes_mid_conv_system() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::MidConvSystem("be brief".into())],
    }];
    let (body, adjustments) = adapter.build_request_body(&messages, &[], &default_options());
    assert!(adjustments.is_empty());
    let block = &body["messages"][0]["content"][0];
    assert_eq!(block["type"], "mid_conv_system");
    assert_eq!(block["text"], "be brief");
}

#[test]
fn audio_block_drops_with_adjustment() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Audio {
            source: MediaSource::Url {
                url: "https://example.com/a.mp3".into(),
            },
        }],
    }];
    let (body, adjustments) = adapter.build_request_body(&messages, &[], &default_options());
    assert_eq!(body["messages"][0]["content"].as_array().unwrap().len(), 0);
    assert_eq!(adjustments.len(), 1);
    assert_eq!(
        adjustments[0].reason,
        "minimax_audio_block_unsupported_in_llm_api"
    );
}

#[test]
fn minimax_only_roles_serialize_actual_strings() {
    let adapter = make_adapter("http://localhost");
    for (role, expected) in [
        (Role::UserSystem, "user_system"),
        (Role::Group, "group"),
        (Role::SampleMessageUser, "sample_message_user"),
        (Role::SampleMessageAi, "sample_message_ai"),
    ] {
        let messages = vec![Message {
            role,
            content: vec![ContentBlock::Text("hi".into())],
        }];
        let (body, adjustments) = adapter.build_request_body(&messages, &[], &default_options());
        assert_eq!(body["messages"][0]["role"], expected, "{role:?}");
        assert!(
            adjustments.is_empty(),
            "Minimax must NOT record OptionAdjustment for its own role: {role:?}"
        );
    }
}

#[test]
fn service_tier_appears_in_body() {
    let adapter = make_adapter("http://localhost");
    let opts = RequestOptions {
        service_tier: Some("priority".into()),
        ..default_options()
    };
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text("hi".into())],
    }];
    let (body, _) = adapter.build_request_body(&messages, &[], &opts);
    assert_eq!(body["service_tier"], "priority");
}

#[test]
fn service_tier_absent_when_none() {
    let adapter = make_adapter("http://localhost");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text("hi".into())],
    }];
    let (body, _) = adapter.build_request_body(&messages, &[], &default_options());
    assert!(body.get("service_tier").is_none());
}

const SIMPLE_SSE: &str = r#"event: message_start
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

#[tokio::test(flavor = "current_thread")]
async fn complete_consumes_anthropic_compatible_sse() {
    let api_url = serve_sse_once(SIMPLE_SSE).await;
    let adapter = make_adapter(&api_url);
    let response = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("complete should succeed");
    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert!(
        matches!(&response.content[0], ContentBlock::Text(t) if t == "Hello"),
        "first block should be text 'Hello': {:?}",
        response.content
    );
    assert_eq!(response.usage.input_tokens, 10);
    assert_eq!(response.usage.output_tokens, 5);
}

#[test]
fn registry_includes_minimax() {
    let registry = crate::registry::ProviderRegistry::new();
    let names = registry.supported_providers();
    assert!(
        names.contains(&"minimax"),
        "minimax should be a registered provider: {names:?}"
    );
    let factory = registry.get("minimax").expect("minimax factory");
    assert_eq!(factory.provider_name(), "minimax");
    assert_eq!(factory.default_api_key_env(), "MINIMAX_API_KEY");
}

// ---------------------------------------------------------------------------
// ADR-0002 slice 006: Minimax on MessagesProtocolFactory + MinimaxProfile.
// ---------------------------------------------------------------------------

use crate::protocol::{Protocol, ProviderProfile};

#[test]
fn minimax_migrated_to_messages_entry_with_profile_and_path_override() {
    let entry = crate::protocol::provider_entry("minimax").expect("minimax migrated");
    assert_eq!(entry.protocols, &[Protocol::Messages]);
    assert!(entry.profile_for(Protocol::Messages).is_some());
    // path_overrides declares the non-standard Anthropic-compatible endpoint.
    assert_eq!(
        entry.path_overrides,
        &[(Protocol::Messages, "/anthropic/v1/messages")]
    );
}

#[test]
fn map_role_emits_native_minimax_roles() {
    let adapter = make_adapter("http://localhost");
    let cx = adapter.cx();
    for (role, expected) in [
        (Role::User, "user"),
        (Role::Tool, "user"),
        (Role::Assistant, "assistant"),
        (Role::UserSystem, "user_system"),
        (Role::Group, "group"),
        (Role::SampleMessageUser, "sample_message_user"),
        (Role::SampleMessageAi, "sample_message_ai"),
    ] {
        assert_eq!(MINIMAX_PROFILE.map_role(&cx, &role).0, expected, "{role:?}");
    }
}

#[test]
fn minimax_hits_anthropic_compatible_path() {
    // Base URL gets Minimax's /anthropic/v1/messages path (not canonical /v1/messages).
    let adapter = make_adapter("https://api.minimaxi.com");
    assert!(
        adapter.api_url.ends_with("/anthropic/v1/messages"),
        "got {}",
        adapter.api_url
    );
}

#[tokio::test]
async fn create_adapter_from_config_routes_minimax_through_new_path() {
    let api_url = serve_sse_once(SIMPLE_SSE).await;

    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "minimax/MiniMax-M3".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: Some(128),
    })
    .expect("minimax resolves through the MessagesProtocolFactory path");

    assert_eq!(adapter.provider_name(), "minimax");
    assert_eq!(adapter.model_name(), "MiniMax-M3");

    let _ = adapter
        .complete(&[], &[], &default_options(), None)
        .await
        .expect("request should complete");
}
