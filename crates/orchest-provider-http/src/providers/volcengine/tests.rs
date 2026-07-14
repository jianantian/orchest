use super::*;
use crate::chat::ChatAdapter;
use crate::{RequestOptions, ThinkingLevel};

fn adapter_with_url(api_url: &str) -> ChatAdapter {
    ChatAdapter::for_test("volcengine", "doubao-seed-2-1-turbo-260628", api_url, 4096)
}

#[test]
fn strips_volcengine_prefix_from_model() {
    // The provider prefix is stripped by the parser.
    let adapter = crate::create_adapter_from_config(crate::ProviderRuntimeConfig {
        model: "volcengine/doubao-seed-2-1-turbo-260628".into(),
        api_key: Some("key".into()),
        api_key_env: None,
        api_url: Some("http://localhost".into()),
        max_tokens: Some(4096),
    })
    .unwrap();
    assert_eq!(adapter.model_name(), "doubao-seed-2-1-turbo-260628");
}

#[test]
fn normalize_url_appends_chat_completions() {
    // normalize_chat_url is brought in via `use super::*` (re-imported in mod.rs)
    assert_eq!(
        super::request::normalize_chat_url("https://ark.cn-beijing.volces.com/api/v3"),
        "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
    );
    assert_eq!(
        super::request::normalize_chat_url(
            "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
        ),
        "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
    );
}

#[test]
fn doubao_seed_supports_thinking() {
    let adapter = adapter_with_url("http://localhost");
    assert!(adapter.capabilities().reasoning.supported);
}

#[test]
fn thinking_enabled_in_request_body() {
    let adapter = adapter_with_url("http://localhost");
    let (body, _) = adapter
        .request_body_for_test(
            &[],
            &[],
            &RequestOptions {
                thinking: ThinkingLevel::High,
                ..Default::default()
            },
        )
        .expect("body");
    assert_eq!(body["thinking"]["type"], "enabled");
}

#[test]
fn thinking_disabled_in_request_body() {
    let adapter = adapter_with_url("http://localhost");
    let (body, _) = adapter
        .request_body_for_test(
            &[],
            &[],
            &RequestOptions {
                thinking: ThinkingLevel::Off,
                ..Default::default()
            },
        )
        .expect("body");
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
        let (body, adjustments) = adapter
            .request_body_for_test(&messages, &[], &RequestOptions::default())
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
// ADR-0002 slice 003: Volcengine on ChatProtocolFactory + VolcengineProfile.
// ---------------------------------------------------------------------------

use crate::protocol::{
    provider_entry, resolve_chat_preflight, OptionSupport, Protocol, ProviderProfile,
    RequestOption, ResolvedModel,
};
use crate::CompatibilityPolicy;

fn cx() -> ResolvedModel<'static> {
    ResolvedModel {
        provider: provider_entry("volcengine").unwrap(),
        protocol: Protocol::Chat,
        model: "doubao-seed-2-1-turbo-260628",
        catalog: crate::catalog::find_model("doubao-seed-2-1-turbo-260628"),
    }
}

#[test]
fn option_support_declares_reasoning_exclusion_unsupported() {
    let support = VOLCENGINE_PROFILE.option_support(&cx(), RequestOption::ReasoningOutputExclusion);
    assert!(matches!(support, OptionSupport::Unsupported { .. }));
}

#[test]
fn reasoning_exclusion_strict_errors_via_preflight() {
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        compatibility_policy: CompatibilityPolicy::Strict,
        ..Default::default()
    };
    let err = resolve_chat_preflight(&VOLCENGINE_PROFILE, &cx(), &opts)
        .expect_err("strict + unsupported exclusion must error");
    assert_eq!(
        err.code.as_deref(),
        Some("unsupported_reasoning_output_exclusion")
    );
    assert_eq!(err.provider.as_deref(), Some("volcengine"));
}

#[test]
fn reasoning_exclusion_coerce_degrades_via_preflight() {
    let opts = RequestOptions {
        thinking: ThinkingLevel::High,
        include_thinking: false,
        compatibility_policy: CompatibilityPolicy::Coerce,
        ..Default::default()
    };
    let (effective, adjustments) = resolve_chat_preflight(&VOLCENGINE_PROFILE, &cx(), &opts)
        .expect("coerce degrades rather than errors");
    assert_eq!(effective.thinking, ThinkingLevel::Off);
    assert!(adjustments
        .iter()
        .any(|a| a.reason == "thinking_disabled_for_output_exclusion"));
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
