use super::*;

fn adapter_with_url(api_url: &str) -> VolcengineAdapter {
    VolcengineAdapter::from_config(VolcengineConfig {
        model: "doubao-seed-2-1-turbo-260628".into(),
        max_tokens: 4096,
        api_key: Some("test-key".into()),
        api_url: Some(api_url.into()),
    })
    .unwrap()
}

#[test]
fn strips_volcengine_prefix_from_model() {
    let adapter = VolcengineAdapter::from_config(VolcengineConfig {
        model: "volcengine/doubao-seed-2-1-turbo-260628".into(),
        max_tokens: 4096,
        api_key: Some("key".into()),
        api_url: Some("http://localhost".into()),
    })
    .unwrap();
    assert_eq!(adapter.model_name(), "doubao-seed-2-1-turbo-260628");
}

#[test]
fn normalize_url_appends_chat_completions() {
    // normalize_chat_url is brought in via `use super::*` (re-imported in mod.rs)
    assert_eq!(
        normalize_chat_url("https://ark.cn-beijing.volces.com/api/v3"),
        "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
    );
    assert_eq!(
        normalize_chat_url("https://ark.cn-beijing.volces.com/api/v3/chat/completions"),
        "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
    );
}

#[test]
fn doubao_seed_supports_thinking() {
    let adapter = adapter_with_url("http://localhost");
    assert!(adapter.supports_thinking());
}

#[test]
fn thinking_enabled_in_request_body() {
    let adapter = adapter_with_url("http://localhost");
    let (body, _) = adapter.build_request_body(
        &[],
        &[],
        &RequestOptions {
            thinking: ThinkingLevel::High,
            ..Default::default()
        },
        true,
    );
    assert_eq!(body["thinking"]["type"], "enabled");
}

#[test]
fn thinking_disabled_in_request_body() {
    let adapter = adapter_with_url("http://localhost");
    let (body, _) = adapter.build_request_body(
        &[],
        &[],
        &RequestOptions {
            thinking: ThinkingLevel::Off,
            ..Default::default()
        },
        false,
    );
    assert_eq!(body["thinking"]["type"], "disabled");
}

#[test]
fn provider_name_is_volcengine() {
    let adapter = adapter_with_url("http://localhost");
    assert_eq!(adapter.provider_name(), "volcengine");
}

#[test]
fn capabilities_has_streaming_and_tool_use() {
    let adapter = adapter_with_url("http://localhost");
    let caps = adapter.capabilities();
    assert!(caps.streaming);
    assert!(caps.tool_use);
    assert!(caps.reasoning.supported);
}

#[test]
fn volcengine_downgrades_minimax_only_roles_with_adjustment() {
    let adapter = adapter_with_url("http://localhost");
    for (role, expected_api_role) in [
        (crate::Role::UserSystem, "system"),
        (crate::Role::Group, "user"),
        (crate::Role::SampleMessageUser, "user"),
        (crate::Role::SampleMessageAi, "user"),
    ] {
        let messages = vec![crate::Message {
            role,
            content: vec![crate::ContentBlock::Text("hi".into())],
        }];
        let (body, adjustments) =
            adapter.build_request_body(&messages, &[], &RequestOptions::default(), false);
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
// ADR-0002 slice 003: Volcengine on ChatProtocolFactory + VolcengineProfile.
// ---------------------------------------------------------------------------

use crate::protocol::{resolve_reasoning_exclusion, OptionSupport, ProviderProfile, RequestOption};
use crate::CompatibilityPolicy;

#[test]
fn option_support_declares_reasoning_exclusion_unsupported() {
    let adapter = adapter_with_url("http://localhost");
    let support =
        VOLCENGINE_PROFILE.option_support(&adapter.cx(), RequestOption::ReasoningOutputExclusion);
    assert!(matches!(support, OptionSupport::Unsupported { .. }));
}

#[test]
fn reasoning_exclusion_strict_errors_via_shared_handler() {
    let adapter = adapter_with_url("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        compatibility_policy: CompatibilityPolicy::Strict,
        ..Default::default()
    };
    let err = resolve_reasoning_exclusion(adapter.profile, &adapter.cx(), &opts, true)
        .expect_err("strict + unsupported exclusion must error");
    assert_eq!(
        err.code.as_deref(),
        Some("unsupported_reasoning_output_exclusion")
    );
    assert_eq!(err.provider.as_deref(), Some("volcengine"));
}

#[test]
fn reasoning_exclusion_coerce_degrades_via_shared_handler() {
    let adapter = adapter_with_url("http://localhost");
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        compatibility_policy: CompatibilityPolicy::Coerce,
        ..Default::default()
    };
    let (effective, adjustment) =
        resolve_reasoning_exclusion(adapter.profile, &adapter.cx(), &opts, true)
            .expect("coerce degrades rather than errors");
    assert!(!effective, "thinking disabled to satisfy exclusion");
    assert_eq!(
        adjustment.expect("degradation recorded").reason,
        "thinking_disabled_for_output_exclusion"
    );
}

#[tokio::test]
async fn create_adapter_from_config_routes_volcengine_through_new_path() {
    let api_url = crate::providers::anthropic::test_util::serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}

data: [DONE]

"#,
    )
    .await;

    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "volcengine/doubao-seed-2-1-turbo-260628".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: Some(128),
    })
    .expect("volcengine resolves through the protocol-factory path");

    assert_eq!(adapter.provider_name(), "volcengine");
    let response = adapter
        .complete(&[], &[], &RequestOptions::default(), None)
        .await
        .expect("request should complete");
    assert!(matches!(&response.content[0], crate::ContentBlock::Text(t) if t == "hi"));
}
