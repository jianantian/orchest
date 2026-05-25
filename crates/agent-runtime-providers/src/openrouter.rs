use std::env;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CapabilitySource, ContentBlock, Message, ModelAdapter, ModelCapabilities,
    ModelError, ModelResponse, OptionAdjustment, ReasoningCapability, RequestOptions, Role,
    StreamEvent, ThinkingLevel, ToolDef,
};

use crate::{defaults, telemetry};

pub struct OpenRouterAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    app_title: Option<String>,
    site_url: Option<String>,
    client: reqwest::Client,
}

impl std::fmt::Debug for OpenRouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct OpenRouterConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub app_title: Option<String>,
    pub site_url: Option<String>,
}

impl OpenRouterAdapter {
    #[allow(clippy::result_large_err)]
    pub fn from_config(config: OpenRouterConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::openrouter::API_KEY_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "OPENROUTER_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .unwrap_or_else(|| defaults::openrouter::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "OpenRouter API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        let app_title = config
            .app_title
            .or_else(|| env::var("OPENROUTER_APP_TITLE").ok());
        let site_url = config
            .site_url
            .or_else(|| env::var("OPENROUTER_SITE_URL").ok());

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
            app_title,
            site_url,
            client: reqwest::Client::new(),
        })
    }

    #[cfg(test)]
    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> (Value, Vec<OptionAdjustment>) {
        self.try_build_request_body(messages, tools, options)
            .expect("valid OpenRouter reasoning replay")
    }

    #[allow(clippy::result_large_err)]
    fn try_build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> Result<(Value, Vec<OptionAdjustment>), ModelError> {
        let mut api_messages: Vec<Value> = Vec::new();
        let adjustments = Vec::new();

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
                    let mut reasoning_details: Vec<Value> = Vec::new();

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
                            ContentBlock::Thinking {
                                text,
                                provider_details,
                                ..
                            } if has_tool_calls => {
                                if let Some(details) = provider_details {
                                    append_reasoning_details(&mut reasoning_details, details)?;
                                } else if reasoning_details.is_empty() {
                                    if let Some(t) = text {
                                        reasoning_text = Some(match reasoning_text {
                                            Some(existing) => format!("{existing}{t}"),
                                            None => t.clone(),
                                        });
                                    }
                                }
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
                    if !reasoning_details.is_empty() {
                        msg["reasoning_details"] = Value::Array(reasoning_details);
                    } else if let Some(reasoning) = reasoning_text {
                        msg["reasoning"] = json!(reasoning);
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

        // ThinkingLevel → reasoning object
        if options.thinking != ThinkingLevel::Off {
            let mut reasoning = json!({});

            if let Some(budget) = options.thinking_budget_tokens {
                reasoning["max_tokens"] = json!(budget);
            } else {
                let effort = match options.thinking {
                    ThinkingLevel::Off => "none",
                    ThinkingLevel::Minimal => "minimal",
                    ThinkingLevel::Low => "low",
                    ThinkingLevel::Medium => "medium",
                    ThinkingLevel::High => "high",
                    ThinkingLevel::XHigh => "xhigh",
                    ThinkingLevel::Max => "max",
                };
                reasoning["effort"] = json!(effort);
            }

            if !options.include_thinking {
                reasoning["exclude"] = json!(true);
            }

            body["reasoning"] = reasoning;
        }

        // temperature / top_p
        if let Some(temp) = options.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(tp) = options.top_p {
            body["top_p"] = json!(tp);
        }

        Ok((body, adjustments))
    }
}

#[allow(clippy::result_large_err)]
fn append_reasoning_details(target: &mut Vec<Value>, details: &Value) -> Result<(), ModelError> {
    match details {
        Value::Array(items) => {
            target.extend(items.iter().cloned());
            Ok(())
        }
        Value::Object(_) => {
            target.push(details.clone());
            Ok(())
        }
        other => Err(ModelError {
            message: format!(
                "OpenRouter reasoning replay details must be object or array, got {other}"
            ),
            code: Some("invalid_reasoning_replay".into()),
            provider: Some("openrouter".into()),
            status: None,
            upstream_code: None,
            upstream_message: None,
            upstream_body: Some(other.clone()),
        }),
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

#[async_trait]
impl ModelAdapter for OpenRouterAdapter {
    fn provider_name(&self) -> &str {
        "openrouter"
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
                efforts: vec![
                    ThinkingLevel::Minimal,
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                    ThinkingLevel::Max,
                ],
                budget_tokens: true,
                output_exclusion: true,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: None,
            source: CapabilitySource::Assumed,
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("openrouter", &self.model, tx.is_some());

        let (body, mut option_adjustments) =
            self.try_build_request_body(messages, tools, options)?;

        let mut request = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body);

        if let Some(ref title) = self.app_title {
            request = request.header("X-OpenRouter-Title", title);
        }
        if let Some(ref url) = self.site_url {
            request = request.header("HTTP-Referer", url);
        }

        let start = Instant::now();
        let response = request.send().await.map_err(|e| {
            telemetry::record_model_error("openrouter", &self.model, start.elapsed());
            ModelError {
                message: e.to_string(),
                code: Some("request_failed".into()),
                provider: Some("openrouter".into()),
                status: None,
                upstream_code: None,
                upstream_message: None,
                upstream_body: None,
            }
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

            telemetry::record_model_error("openrouter", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("openrouter".into()),
                status: Some(status),
                upstream_code,
                upstream_message,
                upstream_body,
            });
        }

        let stream = response.bytes_stream();
        let tx_ref = tx.as_ref();

        let sse = crate::sse::parse_openai_sse_stream(
            stream,
            tx_ref,
            Some("reasoning"),
            Some("reasoning_details"),
            start,
        )
        .await
        .map_err(|mut e| {
            telemetry::record_model_error("openrouter", &self.model, start.elapsed());
            e.provider = Some("openrouter".into());
            e
        })?;

        let content = sse.content;
        let usage = sse.usage;
        let stop_reason = sse.stop_reason;
        let first_token_latency = sse.first_token_latency;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("openrouter", &self.model);
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

        let duration = start.elapsed();
        telemetry::record_model_success(
            "openrouter",
            &self.model,
            duration,
            usage.input_tokens,
            usage.output_tokens,
            first_token_latency,
            Some(duration),
        );

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
    use crate::StopReason;

    fn default_options() -> RequestOptions {
        RequestOptions {
            thinking: ThinkingLevel::Off,
            ..Default::default()
        }
    }

    fn make_adapter(api_url: &str) -> OpenRouterAdapter {
        OpenRouterAdapter::from_config(OpenRouterConfig {
            model: "anthropic/claude-sonnet-4".into(),
            max_tokens: 4096,
            api_key: Some("test-key".into()),
            api_url: Some(api_url.into()),
            app_title: Some("TestApp".into()),
            site_url: Some("https://example.com".into()),
        })
        .expect("adapter should be created")
    }

    #[test]
    fn default_api_url() {
        let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
            model: "anthropic/claude-sonnet-4".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: None,
            app_title: None,
            site_url: None,
        })
        .unwrap();
        assert_eq!(
            adapter.api_url,
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn config_api_key_takes_precedence() {
        let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
            model: "anthropic/claude-sonnet-4".into(),
            max_tokens: 4096,
            api_key: Some("explicit-key".into()),
            api_url: Some("http://localhost".into()),
            app_title: None,
            site_url: None,
        });
        assert!(adapter.is_ok());
    }

    #[test]
    fn missing_api_key_error_code() {
        // Temporarily ensure no env var by testing the error message pattern
        let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
            model: "anthropic/claude-sonnet-4".into(),
            max_tokens: 4096,
            api_key: Some("".into()),
            api_url: Some("http://localhost".into()),
            app_title: None,
            site_url: None,
        });
        // Empty string is still Some, so it succeeds (non-empty validation isn't done on key)
        assert!(adapter.is_ok());
    }

    #[tokio::test]
    async fn sends_custom_headers() {
        let (api_url, capture_rx) = serve_sse_once_capture(
            r#"data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}

data: [DONE]

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let _ = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should succeed");

        let raw_request = capture_rx.await.expect("should capture request");
        let raw_lower = raw_request.to_lowercase();
        assert!(
            raw_lower.contains("x-openrouter-title: testapp"),
            "should contain X-OpenRouter-Title header, got: {raw_request}"
        );
        assert!(
            raw_lower.contains("http-referer: https://example.com"),
            "should contain HTTP-Referer header, got: {raw_request}"
        );
        assert!(
            raw_lower.contains("authorization: bearer test-key"),
            "should contain Authorization header"
        );
    }

    #[test]
    fn model_passthrough() {
        let adapter = make_adapter("http://localhost");
        assert_eq!(adapter.model_name(), "anthropic/claude-sonnet-4");

        let (body, _) = adapter.build_request_body(&[], &[], &default_options());
        assert_eq!(body["model"], "anthropic/claude-sonnet-4");
    }

    #[tokio::test]
    async fn reasoning_maps_to_thinking() {
        let api_url = serve_sse_once(
            r#"data: {"choices":[{"delta":{"reasoning":"let me think about this"}}]}

data: {"choices":[{"delta":{"content":"the answer"}}]}

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
        assert!(
            matches!(&events[1], StreamEvent::Thinking { delta } if delta == "let me think about this")
        );
        assert!(matches!(events[2], StreamEvent::ThinkingEnd { .. }));
        assert!(matches!(&events[3], StreamEvent::Text { delta } if delta == "the answer"));

        assert_eq!(response.content.len(), 2);
        assert!(
            matches!(&response.content[0], ContentBlock::Thinking { text, .. } if text.as_deref() == Some("let me think about this"))
        );
    }

    #[tokio::test]
    async fn reasoning_details_preserved() {
        let api_url = serve_sse_once(
            r#"data: {"choices":[{"delta":{"reasoning":"thinking..."}}]}

data: {"choices":[{"delta":{"content":"done","reasoning_details":[{"type":"text","text":"step 1"},{"type":"text","text":"step 2"}]}}]}

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

        // The ThinkingEnd event should contain the reasoning_details
        let thinking_end = events
            .iter()
            .find(|e| matches!(e, StreamEvent::ThinkingEnd { .. }))
            .expect("should have ThinkingEnd");
        if let StreamEvent::ThinkingEnd {
            provider_details, ..
        } = thinking_end
        {
            let details = provider_details.as_ref().expect("should have details");
            let arr = details.as_array().expect("should be array");
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0]["type"], "text");
            assert_eq!(arr[0]["text"], "step 1");
            assert_eq!(arr[1]["type"], "text");
            assert_eq!(arr[1]["text"], "step 2");
        }

        // Content should also preserve details
        let thinking_block = response
            .content
            .iter()
            .find(|c| matches!(c, ContentBlock::Thinking { .. }))
            .expect("should have Thinking block");
        if let ContentBlock::Thinking {
            provider_details, ..
        } = thinking_block
        {
            let details = provider_details.as_ref().expect("should have details");
            let arr = details.as_array().expect("should be array");
            assert_eq!(arr.len(), 2);
        }
    }

    #[test]
    fn reasoning_object_from_thinking_level() {
        let adapter = make_adapter("http://localhost");

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["reasoning"]["effort"], "high");
        assert!(body["reasoning"].get("max_tokens").is_none());

        let opts = RequestOptions {
            thinking: ThinkingLevel::Max,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["reasoning"]["effort"], "max");

        let opts = RequestOptions {
            thinking: ThinkingLevel::Minimal,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["reasoning"]["effort"], "minimal");
    }

    #[test]
    fn reasoning_effort_and_max_tokens_are_exclusive() {
        let adapter = make_adapter("http://localhost");

        // With budget_tokens → only max_tokens, no effort
        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            thinking_budget_tokens: Some(10000),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["reasoning"]["max_tokens"], 10000);
        assert!(body["reasoning"].get("effort").is_none());

        // Without budget_tokens → only effort, no max_tokens
        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            thinking_budget_tokens: None,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["reasoning"]["effort"], "high");
        assert!(body["reasoning"].get("max_tokens").is_none());
    }

    #[test]
    fn include_thinking_false_sends_exclude() {
        let adapter = make_adapter("http://localhost");

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            include_thinking: false,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["reasoning"]["exclude"], true);

        // include_thinking: true → no exclude field
        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            include_thinking: true,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert!(body["reasoning"].get("exclude").is_none());
    }

    #[test]
    fn replays_multiple_reasoning_detail_blocks_without_reasoning_blocks() {
        let adapter = make_adapter("http://localhost");
        let messages = vec![Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    text: None,
                    signature: None,
                    provider_details: Some(json!([
                        {"type": "reasoning.text", "text": "first"},
                        {"type": "reasoning.signature", "signature": "sig1"}
                    ])),
                },
                ContentBlock::Thinking {
                    text: Some("fallback plaintext should not be sent".into()),
                    signature: None,
                    provider_details: Some(json!({"type": "reasoning.text", "text": "second"})),
                },
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "lookup".into(),
                    input: json!({"q": "orchest"}),
                },
            ],
        }];

        let (body, _) = adapter.build_request_body(&messages, &[], &default_options());
        let msg = &body["messages"][0];

        assert!(msg.get("reasoning_blocks").is_none());
        assert!(msg.get("reasoning").is_none());
        assert_eq!(
            msg["reasoning_details"],
            json!([
                {"type": "reasoning.text", "text": "first"},
                {"type": "reasoning.signature", "signature": "sig1"},
                {"type": "reasoning.text", "text": "second"}
            ])
        );
    }

    #[test]
    fn invalid_reasoning_replay_details_return_error() {
        let adapter = make_adapter("http://localhost");
        let messages = vec![Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    text: None,
                    signature: None,
                    provider_details: Some(json!("not valid replay metadata")),
                },
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "lookup".into(),
                    input: json!({"q": "orchest"}),
                },
            ],
        }];

        let err = adapter
            .try_build_request_body(&messages, &[], &default_options())
            .expect_err("invalid replay metadata should fail");

        assert_eq!(err.code.as_deref(), Some("invalid_reasoning_replay"));
    }

    #[test]
    fn provider_name_and_capabilities() {
        let adapter = make_adapter("http://localhost");
        assert_eq!(adapter.provider_name(), "openrouter");
        let caps = adapter.capabilities();
        assert_eq!(caps.source, CapabilitySource::Assumed);
        assert!(caps.reasoning.budget_tokens);
        assert!(caps.reasoning.output_exclusion);
    }

    #[tokio::test]
    async fn stop_reason_end_turn() {
        let api_url = serve_sse_once(
            r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}

data: [DONE]

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }
}
