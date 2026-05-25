use std::env;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, Message,
    ModelAdapter, ModelCapabilities, ModelError, ModelResponse, OptionAdjustment,
    ReasoningCapability, RequestOptions, Role, StreamEvent, ThinkingLevel, ToolDef,
};

use crate::{defaults, telemetry};

pub struct OpenAiAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    client: reqwest::Client,
}

impl std::fmt::Debug for OpenAiAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct OpenAiConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl OpenAiAdapter {
    #[allow(clippy::result_large_err)]
    pub fn from_config(config: OpenAiConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "OPENAI_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .or_else(|| env::var("OPENAI_API_URL").ok())
            .or_else(|| env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_else(|| defaults::openai::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "OpenAI API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config
                .model
                .strip_prefix("openai/")
                .unwrap_or(&config.model)
                .to_string(),
            max_tokens: config.max_tokens,
            client: reqwest::Client::new(),
        })
    }

    fn supports_reasoning(&self) -> bool {
        supports_reasoning_model(&self.model)
    }

    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
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
                    let mut text_parts = Vec::new();
                    let mut tool_calls_arr = Vec::new();
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
                            // Skip Thinking blocks — OpenAI doesn't need replay
                            ContentBlock::Thinking { .. } => {}
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

        // ThinkingLevel → reasoning_effort
        if options.thinking != ThinkingLevel::Off && self.supports_reasoning() {
            let effort = match options.thinking {
                ThinkingLevel::Off => unreachable!(),
                ThinkingLevel::Minimal => "low",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
                ThinkingLevel::XHigh => "high",
                ThinkingLevel::Max => "high",
            };
            body["reasoning_effort"] = json!(effort);
        } else if options.thinking != ThinkingLevel::Off {
            adjustments.push(OptionAdjustment {
                option: "thinking".into(),
                requested: json!(format!("{:?}", options.thinking)),
                applied: json!("Off"),
                reason: "unsupported_reasoning_model".into(),
            });
        }

        // thinking_budget_tokens is not supported by OpenAI
        if options.thinking_budget_tokens.is_some() {
            adjustments.push(OptionAdjustment {
                option: "thinking_budget_tokens".into(),
                requested: json!(options.thinking_budget_tokens),
                applied: json!(null),
                reason: "unsupported_by_provider".into(),
            });
        }

        // CachePolicy mapping
        if options.cache_policy == CachePolicy::Long {
            // Only some models support extended cache retention
            // For now, record an adjustment for unsupported models
            adjustments.push(OptionAdjustment {
                option: "cache_policy".into(),
                requested: json!("Long"),
                applied: json!("Auto"),
                reason: "unsupported_cache_retention".into(),
            });
        }

        // temperature / top_p
        if let Some(temp) = options.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(tp) = options.top_p {
            body["top_p"] = json!(tp);
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

fn supports_reasoning_model(model: &str) -> bool {
    matches!(
        model.split_once('/').map_or(model, |(_, model)| model),
        name if name.starts_with("o1")
            || name.starts_with("o3")
            || name.starts_with("o4")
            || name.starts_with("gpt-5")
    )
}

#[async_trait]
impl ModelAdapter for OpenAiAdapter {
    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let supports_reasoning = self.supports_reasoning();

        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: supports_reasoning,
                efforts: if supports_reasoning {
                    vec![
                        ThinkingLevel::Low,
                        ThinkingLevel::Medium,
                        ThinkingLevel::High,
                    ]
                } else {
                    vec![]
                },
                budget_tokens: false,
                output_exclusion: false,
                replay_metadata_required: false,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: Some(128_000),
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
        let _span =
            telemetry::model_complete_span("openai", &self.model, tx.is_some());

        if options.compatibility_policy == CompatibilityPolicy::Strict
            && options.thinking_budget_tokens.is_some()
        {
            return Err(ModelError::internal(
                "thinking_budget_tokens is not supported by OpenAI",
                "unsupported_thinking_budget",
            ));
        }
        if options.compatibility_policy == CompatibilityPolicy::Strict
            && options.thinking != ThinkingLevel::Off
            && !self.supports_reasoning()
        {
            return Err(ModelError::internal(
                format!(
                    "model '{}' does not declare OpenAI reasoning support",
                    self.model
                ),
                "unsupported_reasoning_model",
            ));
        }

        let (body, mut option_adjustments) = self.build_request_body(messages, tools, options);

        let start = Instant::now();
        let response = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("openai", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("openai".into()),
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

            telemetry::record_model_error("openai", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("openai".into()),
                status: Some(status),
                upstream_code,
                upstream_message,
                upstream_body,
            });
        }

        let stream = response.bytes_stream();
        let tx_ref = tx.as_ref();

        let sse = crate::sse::parse_openai_sse_stream(stream, tx_ref, None, None, start)
            .await
            .map_err(|mut e| {
                telemetry::record_model_error("openai", &self.model, start.elapsed());
                e.provider = Some("openai".into());
                e
            })?;

        let content = sse.content;
        let usage = sse.usage;
        let stop_reason = sse.stop_reason;
        let first_token_latency = sse.first_token_latency;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("openai", &self.model);
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
            "openai",
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

    fn make_adapter(api_url: &str) -> OpenAiAdapter {
        OpenAiAdapter::from_config(OpenAiConfig {
            model: "gpt-4o-mini".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url.into()),
        })
        .expect("adapter should be created")
    }

    #[test]
    fn strips_prefix() {
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: "openai/gpt-4o-mini".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost/v1/chat/completions".into()),
        })
        .expect("adapter");
        assert_eq!(adapter.model, "gpt-4o-mini");
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

        let (body, _) = adapter.build_request_body(&messages, &[], &default_options());
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
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: "o3-mini".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
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

        let (body, adjustments) = adapter.build_request_body(&[], &[], &opts);

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
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: "o3-mini".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
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
        let (_, adjustments) = adapter.build_request_body(&[], &[], &opts);
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
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
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
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["max_tokens"], 8192);

        let opts_none = default_options();
        let (body2, _) = adapter.build_request_body(&[], &[], &opts_none);
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
}
