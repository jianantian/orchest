use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// -----------------------------------------------------------------------
// MockAdapter for helper / runtime_contract tests
// -----------------------------------------------------------------------
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    name: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self { name, previous }
    }

    fn remove(name: &'static str) -> Self {
        let previous = std::env::var(name).ok();
        std::env::remove_var(name);
        Self { name, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().expect("env lock poisoned")
}

struct MockAdapter {
    call_count: Arc<AtomicU32>,
}

impl MockAdapter {
    fn new() -> (Self, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Self {
                call_count: count.clone(),
            },
            count,
        )
    }
}

#[async_trait::async_trait]
impl ModelAdapter for MockAdapter {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "mock-model"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::Text {
                    delta: "hello".into(),
                })
                .await;
            let _ = tx
                .send(StreamEvent::Done {
                    usage: TokenUsage {
                        input_tokens: 10,
                        output_tokens: 5,
                        ..Default::default()
                    },
                })
                .await;
        }

        Ok(ModelResponse {
            content: vec![ContentBlock::Text("hello".into())],
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// -----------------------------------------------------------------------
// Factory tests
// -----------------------------------------------------------------------

#[test]
fn factory_routes_by_provider() {
    let anthropic = create_adapter("anthropic/claude-sonnet-4", Some("key".into()))
        .expect("anthropic should create");
    assert_eq!(anthropic.provider_name(), "anthropic");

    let openai =
        create_adapter("openai/gpt-4o", Some("key".into())).expect("openai should create");
    assert_eq!(openai.provider_name(), "openai");

    let deepseek = create_adapter("deepseek/deepseek-chat", Some("key".into()))
        .expect("deepseek should create");
    assert_eq!(deepseek.provider_name(), "deepseek");

    let openrouter = create_adapter("openrouter/anthropic/claude-sonnet-4", Some("key".into()))
        .expect("openrouter should create");
    assert_eq!(openrouter.provider_name(), "openrouter");
}

#[test]
fn factory_rejects_unknown_provider() {
    let result = create_adapter("gemini/gemini-pro", Some("key".into()));
    match result {
        Err(err) => assert_eq!(err.code.as_deref(), Some("unknown_provider")),
        Ok(_) => panic!("should have failed"),
    }
}

#[test]
fn factory_rejects_no_slash() {
    let result = create_adapter("claude-sonnet-4", Some("key".into()));
    let adapter = result.expect("legacy Anthropic shorthand should be accepted");
    assert_eq!(adapter.provider_name(), "anthropic");
    assert_eq!(adapter.model_name(), "claude-sonnet-4");
}

#[test]
fn provider_config_normalizes_legacy_anthropic_model() {
    let normalized = normalize_provider_model("claude-sonnet-4").expect("normalizes");
    assert_eq!(normalized.provider, "anthropic");
    assert_eq!(normalized.model, "claude-sonnet-4");
}

#[test]
fn provider_config_preserves_canonical_provider_model() {
    let normalized = normalize_provider_model("deepseek/deepseek-chat").expect("normalizes");
    assert_eq!(normalized.provider, "deepseek");
    assert_eq!(normalized.model, "deepseek-chat");
}

#[test]
fn provider_config_openrouter_preserves_nested_model_name() {
    let normalized =
        normalize_provider_model("openrouter/anthropic/claude-sonnet-4").expect("normalizes");
    assert_eq!(normalized.provider, "openrouter");
    assert_eq!(normalized.model, "anthropic/claude-sonnet-4");
}

#[test]
fn provider_config_api_key_precedence_explicit_then_env() {
    let _env_lock = lock_env();
    let env_name = "ORCHEST_TEST_PROVIDER_API_KEY_PRECEDENCE";
    let _env_guard = EnvVarGuard::set(env_name, "env-key");

    let registry = ProviderRegistry::new();
    let factory = registry.get("openai").unwrap();
    let explicit = resolve_api_key(factory, Some("explicit-key"), Some(env_name))
        .expect("explicit key wins");
    let from_env = resolve_api_key(factory, None, Some(env_name)).expect("env key resolves");

    assert_eq!(explicit, "explicit-key");
    assert_eq!(from_env, "env-key");
}

#[test]
fn provider_config_api_key_env_override_does_not_fall_back_to_provider_default() {
    let _env_lock = lock_env();
    let local_env_name = "ORCHEST_TEST_MISSING_LOCAL_PROVIDER_API_KEY";
    let _local_env_guard = EnvVarGuard::remove(local_env_name);
    let _openai_env_guard = EnvVarGuard::set("OPENAI_API_KEY", "global-openai-key");

    let registry = ProviderRegistry::new();
    let factory = registry.get("openai").unwrap();
    let err = match resolve_api_key(factory, None, Some(local_env_name)) {
        Ok(_) => {
            panic!("missing local api_key_env should fail instead of using OPENAI_API_KEY")
        }
        Err(err) => err,
    };

    assert_eq!(err.code.as_deref(), Some("missing_api_key"));
}

#[test]
fn provider_config_missing_provider_key_does_not_use_other_provider_env() {
    let _env_lock = lock_env();
    let _openai_env_guard = EnvVarGuard::remove("OPENAI_API_KEY");
    let _anthropic_env_guard = EnvVarGuard::set("ANTHROPIC_API_KEY", "anthropic-key");

    let registry = ProviderRegistry::new();
    let factory = registry.get("openai").unwrap();
    let err = match resolve_api_key(factory, None, None) {
        Ok(_) => panic!("openai config should not use ANTHROPIC_API_KEY"),
        Err(err) => err,
    };

    assert_eq!(err.code.as_deref(), Some("missing_api_key"));
}

#[test]
fn provider_config_api_key_empty_string_rejected() {
    let result = create_adapter_from_config(ProviderRuntimeConfig {
        model: "openai/gpt-4o".into(),
        api_key: Some(" ".into()),
        api_key_env: None,
        api_url: None,
        max_tokens: None,
    });
    let err = match result {
        Ok(_) => panic!("empty api key should fail"),
        Err(err) => err,
    };

    assert_eq!(err.code.as_deref(), Some("invalid_api_key"));
}

#[tokio::test]
async fn provider_config_api_url_override_reaches_adapter_config() {
    let api_url = crate::providers::anthropic::test_util::serve_sse_once(
        r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}

data: [DONE]

"#,
    )
    .await;

    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: "openai/gpt-4o".into(),
        api_key: Some("test-key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: None,
    })
    .expect("adapter should create");

    let response = adapter
        .complete(&[], &[], &RequestOptions::default(), None)
        .await
        .expect("api_url should point to test server");
    assert_eq!(response.usage.input_tokens, 1);
}

#[tokio::test]
async fn provider_config_max_tokens_default_and_override() {
    let (api_url, capture_rx) = serve_openai_sse_capture_full_request().await;

    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: "openai/gpt-4o".into(),
        api_key: Some("test-key".into()),
        api_key_env: None,
        api_url: Some(api_url),
        max_tokens: Some(1234),
    })
    .expect("adapter should create");

    let mut options = RequestOptions::default();
    options.max_tokens = Some(5678);
    let _ = adapter
        .complete(&[], &[], &options, None)
        .await
        .expect("request succeeds");

    let raw_request = capture_rx.await.expect("request captured");
    assert!(
        raw_request.contains(r#""max_tokens":5678"#),
        "request max_tokens should override provider config: {raw_request}"
    );
}

async fn serve_openai_sse_capture_full_request(
) -> (String, tokio::sync::oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test server should bind");
    let address = listener
        .local_addr()
        .expect("test server should have local address");
    let (capture_tx, capture_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("test server should accept one request");
        let request = read_http_request(&mut socket).await;
        let _ = capture_tx.send(request);

        let body = r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}

data: [DONE]

"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("test server should write response");
    });

    (format!("http://{address}"), capture_rx)
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = vec![0; 1024];
    loop {
        let n = socket.read(&mut chunk).await.expect("read");
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
        let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n")
        else {
            continue;
        };
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if buffer.len() >= header_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

#[test]
fn factory_openrouter_preserves_full_model() {
    let adapter = create_adapter("openrouter/anthropic/claude-sonnet-4", Some("key".into()))
        .expect("should create");
    assert_eq!(adapter.model_name(), "anthropic/claude-sonnet-4");
}

// -----------------------------------------------------------------------
// Helper function tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn stream_chat_returns_pair() {
    let (adapter, _count) = MockAdapter::new();
    let messages = [];
    let tools = [];
    let options = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };

    let (future, mut rx) = stream_chat(&adapter, &messages, &tools, &options);
    let response = future.await.expect("should succeed");

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "hello"));
    assert!(events
        .iter()
        .any(|e| matches!(e, StreamEvent::Text { delta } if delta == "hello")));
    assert!(events.iter().any(|e| matches!(e, StreamEvent::Done { .. })));
}

#[tokio::test]
async fn chat_returns_response() {
    let (adapter, _count) = MockAdapter::new();
    let messages = [];
    let tools = [];
    let options = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };

    let response = chat(&adapter, &messages, &tools, &options)
        .await
        .expect("should succeed");
    assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "hello"));
    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

#[tokio::test]
async fn stream_chat_and_chat_are_semantically_equivalent() {
    let (adapter1, _) = MockAdapter::new();
    let (adapter2, _) = MockAdapter::new();
    let messages = [];
    let tools = [];
    let options = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };

    let chat_response = chat(&adapter1, &messages, &tools, &options)
        .await
        .expect("chat should succeed");

    let (future, mut rx) = stream_chat(&adapter2, &messages, &tools, &options);
    let stream_response = future.await.expect("stream_chat should succeed");
    while rx.recv().await.is_some() {}

    assert_eq!(chat_response.content, stream_response.content);
    assert_eq!(chat_response.usage, stream_response.usage);
    assert_eq!(chat_response.stop_reason, stream_response.stop_reason);
}

#[tokio::test]
async fn helpers_use_model_adapter_complete() {
    let (adapter, count) = MockAdapter::new();
    let messages = [];
    let tools = [];
    let options = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };

    let _ = chat(&adapter, &messages, &tools, &options).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);

    let (future, _rx) = stream_chat(&adapter, &messages, &tools, &options);
    let _ = future.await;
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

// -----------------------------------------------------------------------
// Cross-adapter integration tests
// -----------------------------------------------------------------------

#[test]
fn all_adapters_report_normalized_capabilities() {
    let adapters: Vec<Box<dyn ModelAdapter>> = vec![
        Box::new(
            AnthropicAdapter::from_config(AnthropicConfig {
                model: "claude-sonnet-4".into(),
                max_tokens: 4096,
                api_key: Some("key".into()),
                api_url: Some("http://localhost".into()),
            })
            .unwrap(),
        ),
        Box::new(
            OpenAiAdapter::from_config(OpenAiConfig {
                model: "gpt-4o".into(),
                max_tokens: 4096,
                api_key: Some("key".into()),
                api_url: Some("http://localhost".into()),
            })
            .unwrap(),
        ),
        Box::new(
            DeepSeekAdapter::from_config(DeepSeekConfig {
                model: "deepseek-chat".into(),
                max_tokens: 4096,
                api_key: Some("key".into()),
                api_url: Some("http://localhost".into()),
            })
            .unwrap(),
        ),
        Box::new(
            OpenRouterAdapter::from_config(OpenRouterConfig {
                model: "anthropic/claude-sonnet-4".into(),
                max_tokens: 4096,
                api_key: Some("key".into()),
                api_url: Some("http://localhost".into()),
                app_title: None,
                site_url: None,
            })
            .unwrap(),
        ),
    ];

    for adapter in &adapters {
        let caps = adapter.capabilities();
        assert!(
            caps.streaming,
            "{} should support streaming",
            adapter.provider_name()
        );
        assert!(
            caps.tool_use,
            "{} should support tool_use",
            adapter.provider_name()
        );
        let _ = caps.reasoning.efforts;
        let _ = caps.prompt_cache.supported;
    }
}

#[test]
fn provider_reasons_are_not_lost() {
    let stop_reasons = vec![
        StopReason::EndTurn,
        StopReason::ToolUse,
        StopReason::MaxTokens,
        StopReason::StopSequence,
        StopReason::ContentFilter,
        StopReason::Refusal,
        StopReason::ContextWindowExceeded,
        StopReason::Pause,
        StopReason::Interrupted,
        StopReason::Other("custom".into()),
    ];

    for reason in &stop_reasons {
        let json = serde_json::to_string(reason).unwrap();
        let restored: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(*reason, restored);
    }
}

#[test]
fn system_role_maps_per_provider() {
    let adapter = OpenAiAdapter::from_config(OpenAiConfig {
        model: "gpt-4o".into(),
        max_tokens: 4096,
        api_key: Some("key".into()),
        api_url: Some("http://localhost".into()),
    })
    .unwrap();

    let messages = vec![Message {
        role: Role::System,
        content: vec![ContentBlock::Text("You are helpful.".into())],
    }];
    let options = RequestOptions {
        thinking: ThinkingLevel::Off,
        ..Default::default()
    };

    assert_eq!(adapter.provider_name(), "openai");
    assert!(!messages.is_empty());
    assert_eq!(options.thinking, ThinkingLevel::Off);
}

#[test]
fn tool_results_map_per_provider() {
    let tool_result = ContentBlock::ToolResult {
        tool_use_id: "call_1".into(),
        content: json!({"result": "success"}),
    };
    let json = serde_json::to_string(&tool_result).unwrap();
    let restored: ContentBlock = serde_json::from_str(&json).unwrap();
    assert_eq!(tool_result, restored);
}

#[test]
fn coerce_reports_adjustment() {
    let adj = OptionAdjustment {
        option: "thinking_budget_tokens".into(),
        requested: json!(10000),
        applied: json!(null),
        reason: "unsupported_by_provider".into(),
    };
    let json = serde_json::to_string(&adj).unwrap();
    let restored: OptionAdjustment = serde_json::from_str(&json).unwrap();
    assert_eq!(adj, restored);
}

#[test]
fn registry_lists_all_built_in_providers() {
    let registry = ProviderRegistry::new();
    let providers = registry.supported_providers();
    assert!(providers.contains(&"anthropic"));
    assert!(providers.contains(&"openai"));
    assert!(providers.contains(&"deepseek"));
    assert!(providers.contains(&"openrouter"));
    assert!(providers.contains(&"volcengine"));
}
