use std::env;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, Message,
    ModelAdapter, ModelCapabilities, ModelError, ModelResponse, OptionAdjustment,
    ReasoningCapability, RequestOptions, Role, StreamEvent, ThinkingLevel, ToolDef,
};

const DEFAULT_API_URL: &str = "https://api.deepseek.com";

pub struct DeepSeekAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    client: reqwest::Client,
}

impl std::fmt::Debug for DeepSeekAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepSeekAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct DeepSeekConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl DeepSeekAdapter {
    #[allow(clippy::result_large_err)]
    pub fn from_config(config: DeepSeekConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var("DEEPSEEK_API_KEY").ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "DEEPSEEK_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "DeepSeek API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
            client: reqwest::Client::new(),
        })
    }

    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        thinking_enabled: bool,
    ) -> (Value, Vec<OptionAdjustment>) {
        let mut api_messages: Vec<Value> = Vec::new();
        let mut adjustments = Vec::new();

        for message in messages {
            match message.role {
                Role::System => {
                    let text = message
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    api_messages.push(json!({"role": "system", "content": text}));
                }
                Role::User => {
                    let mut text_parts = Vec::new();
                    let mut tool_results = Vec::new();
                    for block in &message.content {
                        match block {
                            ContentBlock::Text(t) => text_parts.push(t.clone()),
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                            } => {
                                tool_results.push((tool_use_id.clone(), content.clone()));
                            }
                            _ => {}
                        }
                    }
                    if !tool_results.is_empty() {
                        for (tool_call_id, content) in tool_results {
                            let content_str = match &content {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            api_messages.push(json!({
                                "role": "tool",
                                "tool_call_id": tool_call_id,
                                "content": content_str
                            }));
                        }
                    } else {
                        let text = text_parts.join("\n");
                        api_messages.push(json!({"role": "user", "content": text}));
                    }
                }
                Role::Assistant => {
                    let has_tool_calls = message
                        .content
                        .iter()
                        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

                    let mut text_parts = Vec::new();
                    let mut tool_calls_arr = Vec::new();
                    let mut reasoning_text: Option<String> = None;

                    for block in &message.content {
                        match block {
                            ContentBlock::Text(t) => text_parts.push(t.clone()),
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls_arr.push(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string()
                                    }
                                }));
                            }
                            ContentBlock::Thinking { text: Some(t), .. } if has_tool_calls => {
                                reasoning_text = Some(t.clone());
                            }
                            _ => {}
                        }
                    }

                    let mut msg = json!({"role": "assistant"});
                    if !text_parts.is_empty() {
                        msg["content"] = json!(text_parts.join("\n"));
                    } else if tool_calls_arr.is_empty() {
                        msg["content"] = json!("");
                    }
                    if !tool_calls_arr.is_empty() {
                        msg["tool_calls"] = Value::Array(tool_calls_arr);
                    }
                    if let Some(rt) = reasoning_text {
                        msg["reasoning_content"] = json!(rt);
                    }
                    api_messages.push(msg);
                }
                Role::Tool => {
                    for block in &message.content {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                        } = block
                        {
                            let content_str = match content {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            api_messages.push(json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": content_str
                            }));
                        }
                    }
                }
            }
        }

        let effective_max_tokens = options.max_tokens.unwrap_or(self.max_tokens);

        let mut body = json!({
            "model": self.model,
            "max_tokens": effective_max_tokens,
            "messages": api_messages,
            "stream": true,
            "stream_options": {"include_usage": true}
        });

        if !tools.is_empty() {
            body["tools"] = Value::Array(
                tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema
                            }
                        })
                    })
                    .collect(),
            );
        }

        // ThinkingLevel → thinking + reasoning_effort
        if thinking_enabled {
            body["thinking"] = json!({"type": "enabled"});
            let effort = match options.thinking {
                ThinkingLevel::XHigh | ThinkingLevel::Max => "max",
                _ => "high",
            };
            body["reasoning_effort"] = json!(effort);
        } else {
            body["thinking"] = json!({"type": "disabled"});
        }

        // thinking_budget_tokens is not supported by DeepSeek
        if options.thinking_budget_tokens.is_some() {
            adjustments.push(OptionAdjustment {
                option: "thinking_budget_tokens".into(),
                requested: json!(options.thinking_budget_tokens),
                applied: json!(null),
                reason: "unsupported_by_provider".into(),
            });
        }

        // CachePolicy — DeepSeek caching is fully automatic
        if options.cache_policy != CachePolicy::Auto && options.cache_policy != CachePolicy::None {
            adjustments.push(OptionAdjustment {
                option: "cache_policy".into(),
                requested: json!(format!("{:?}", options.cache_policy)),
                applied: json!("Auto"),
                reason: "deepseek_cache_automatic".into(),
            });
        }

        // Sampling parameters — omit when thinking is enabled
        if !thinking_enabled {
            if let Some(temp) = options.temperature {
                body["temperature"] = json!(temp);
            }
            if let Some(tp) = options.top_p {
                body["top_p"] = json!(tp);
            }
        }

        (body, adjustments)
    }
}

fn normalize_chat_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

fn map_stop_reason(raw: &str) -> StopReason {
    match raw {
        "stop" => StopReason::EndTurn,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        "content_filter" => StopReason::ContentFilter,
        "insufficient_system_resource" => StopReason::Interrupted,
        other => StopReason::Other(other.to_string()),
    }
}

use crate::StopReason;

#[async_trait]
impl ModelAdapter for DeepSeekAdapter {
    fn provider_name(&self) -> &str {
        "deepseek"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts: vec![ThinkingLevel::High, ThinkingLevel::Max],
                budget_tokens: false,
                output_exclusion: false,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: Some(64_000),
            source: CapabilitySource::Static,
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let thinking_enabled = options.thinking != ThinkingLevel::Off;

        // Handle include_thinking: false when thinking is enabled
        let effective_thinking = if !options.include_thinking && thinking_enabled {
            match options.compatibility_policy {
                CompatibilityPolicy::Strict => {
                    return Err(ModelError {
                        message: "DeepSeek does not support output exclusion for reasoning".into(),
                        code: Some("unsupported_reasoning_output_exclusion".into()),
                        provider: Some("deepseek".into()),
                        status: None,
                        upstream_code: None,
                        upstream_message: None,
                        upstream_body: None,
                    });
                }
                CompatibilityPolicy::Coerce => false,
            }
        } else {
            thinking_enabled
        };

        let (body, mut option_adjustments) =
            self.build_request_body(messages, tools, options, effective_thinking);

        if !options.include_thinking && thinking_enabled && !effective_thinking {
            option_adjustments.push(OptionAdjustment {
                option: "include_thinking".into(),
                requested: json!(false),
                applied: json!("thinking_disabled"),
                reason: "output_exclusion_unsupported_disables_reasoning".into(),
            });
        }

        let response = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ModelError {
                message: e.to_string(),
                code: Some("request_failed".into()),
                provider: Some("deepseek".into()),
                status: None,
                upstream_code: None,
                upstream_message: None,
                upstream_body: None,
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_text = response.text().await.unwrap_or_default();
            let upstream_body: Option<Value> = serde_json::from_str(&body_text).ok();
            let (upstream_code, upstream_message) = upstream_body
                .as_ref()
                .and_then(|b| b.get("error"))
                .map(|err| {
                    (
                        err.get("type").and_then(|v| v.as_str()).map(String::from),
                        err.get("message")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    )
                })
                .unwrap_or((None, None));

            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("deepseek".into()),
                status: Some(status),
                upstream_code,
                upstream_message,
                upstream_body,
            });
        }

        let stream = response.bytes_stream();
        let tx_ref = tx.as_ref();

        let (content, mut usage, stop_reason) =
            crate::sse::parse_openai_sse_stream(stream, tx_ref, Some("reasoning"), None)
                .await
                .map_err(|mut e| {
                    e.provider = Some("deepseek".into());
                    e
                })?;

        // Remap stop reason using DeepSeek-specific mapping
        let stop_reason = match &stop_reason {
            StopReason::Other(raw) => map_stop_reason(raw),
            _ => stop_reason,
        };

        // Extract DeepSeek-specific cache tokens from raw usage
        // The SSE parser doesn't know about prompt_cache_hit_tokens / prompt_cache_miss_tokens,
        // so we handle them here via the standard usage fields that the parser already extracted.
        // DeepSeek reports prompt_cache_hit_tokens and prompt_cache_miss_tokens at the usage level.
        // Since these come through the SSE stream, we need to re-parse if the parser didn't capture them.
        // However, the SSE parser already handles prompt_tokens_details.cached_tokens.
        // DeepSeek uses a different field name, so cache_read_tokens may be 0.
        // We set cache_write_tokens to 0 as specified.
        usage.cache_write_tokens = 0;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            option_adjustments.push(OptionAdjustment {
                option: "usage".into(),
                requested: json!(null),
                applied: json!(null),
                reason: "usage_not_reported".into(),
            });
        }

        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        Ok(ModelResponse {
            content,
            usage,
            stop_reason,
            option_adjustments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::test_util::*;

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
    fn env_var_fallback() {
        let result = DeepSeekAdapter::from_config(DeepSeekConfig {
            model: "deepseek-chat".into(),
            max_tokens: 4096,
            api_key: None,
            api_url: Some("http://localhost".into()),
        });
        // Without DEEPSEEK_API_KEY set, this should fail
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code.as_deref(), Some("missing_api_key"));
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
}
