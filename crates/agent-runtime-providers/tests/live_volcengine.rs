//! Live acceptance tests against the real Volcengine Ark API.
//!
//! Requires `ARK_API_KEY` (see `.env` at the repo root — `VOLCENGINE_API_KEY`
//! is the same Ark key and works interchangeably). Run with:
//!
//!   cargo test -p agent-runtime-providers --test live_volcengine -- --ignored --nocapture

use std::env;

use agent_runtime_providers::{
    ContentBlock, Message, ModelAdapter, RequestOptions, Role, StopReason, ThinkingLevel,
    VolcengineAdapter, VolcengineConfig,
};

fn load_dotenv_if_present() {
    let mut path = std::env::current_dir().ok();
    let mut contents = None;
    while let Some(dir) = path {
        let candidate = dir.join(".env");
        if let Ok(value) = std::fs::read_to_string(&candidate) {
            contents = Some(value);
            break;
        }
        path = dir.parent().map(|parent| parent.to_path_buf());
    }
    let Some(contents) = contents else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key
            .trim()
            .strip_prefix("export ")
            .unwrap_or(key.trim())
            .trim();
        if env::var_os(key).is_some() {
            continue;
        }
        env::set_var(key, value.trim().trim_matches('"'));
    }
}

fn adapter(model: &str) -> VolcengineAdapter {
    load_dotenv_if_present();
    let api_key = env::var("ARK_API_KEY").expect("ARK_API_KEY must be set for live test");
    VolcengineAdapter::from_config(VolcengineConfig {
        model: model.into(),
        max_tokens: 256,
        api_key: Some(api_key),
        api_url: None,
    })
    .expect("adapter config should be valid")
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_chat_completion_without_thinking() {
    let adapter = adapter("doubao-seed-2-1-turbo-260628");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text(
            "Reply with exactly the word: pong".into(),
        )],
    }];
    let response = adapter
        .complete(
            &messages,
            &[],
            &RequestOptions {
                thinking: ThinkingLevel::Off,
                ..Default::default()
            },
            None,
        )
        .await
        .expect("live chat completion should succeed");

    let text: String = response
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(!text.trim().is_empty(), "expected non-empty text response");
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert!(matches!(
        response.stop_reason,
        StopReason::EndTurn | StopReason::MaxTokens
    ));
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_chat_completion_with_thinking_enabled() {
    let adapter = adapter("doubao-seed-2-1-turbo-260628");
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text(
            "What is 17 * 23? Answer with just the number.".into(),
        )],
    }];
    let response = adapter
        .complete(
            &messages,
            &[],
            &RequestOptions {
                thinking: ThinkingLevel::High,
                include_thinking: true,
                ..Default::default()
            },
            None,
        )
        .await
        .expect("live chat completion with thinking should succeed");

    let has_thinking = response
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Thinking { .. }));
    assert!(
        has_thinking,
        "expected a Thinking content block when thinking is enabled"
    );

    let text: String = response
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains("391"),
        "expected correct arithmetic answer in response: {text}"
    );
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_tool_calling_round_trip() {
    let adapter = adapter("doubao-seed-2-1-turbo-260628");
    let tool = agent_runtime_providers::ToolDef {
        name: "get_weather".into(),
        description: "Get the current weather for a city".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "city": {"type": "string"}
            },
            "required": ["city"]
        }),
    };
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text(
            "What's the weather in Beijing? Use the get_weather tool.".into(),
        )],
    }];
    let response = adapter
        .complete(
            &messages,
            &[tool],
            &RequestOptions {
                thinking: ThinkingLevel::Off,
                ..Default::default()
            },
            None,
        )
        .await
        .expect("live tool calling should succeed");

    let tool_use = response
        .content
        .iter()
        .find(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name == "get_weather"));
    assert!(
        tool_use.is_some(),
        "expected model to call get_weather tool, got: {:?}",
        response.content
    );
    assert!(matches!(response.stop_reason, StopReason::ToolUse));
}
