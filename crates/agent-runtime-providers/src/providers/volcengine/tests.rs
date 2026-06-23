use super::*;

fn adapter_with_url(api_url: &str) -> VolcengineAdapter {
    VolcengineAdapter::from_config(VolcengineConfig {
        model: "doubao-seed-2-0-lite-260215".into(),
        max_tokens: 4096,
        api_key: Some("test-key".into()),
        api_url: Some(api_url.into()),
    })
    .unwrap()
}

#[test]
fn strips_volcengine_prefix_from_model() {
    let adapter = VolcengineAdapter::from_config(VolcengineConfig {
        model: "volcengine/doubao-seed-2-0-lite-260215".into(),
        max_tokens: 4096,
        api_key: Some("key".into()),
        api_url: Some("http://localhost".into()),
    })
    .unwrap();
    assert_eq!(adapter.model_name(), "doubao-seed-2-0-lite-260215");
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
