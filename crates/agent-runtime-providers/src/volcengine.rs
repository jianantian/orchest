//! Volcengine Ark (火山方舟) adapter — OpenAI-compatible Chat Completions.
//!
//! API base: https://ark.cn-beijing.volces.com/api/v3/chat/completions
//! Auth:     Authorization: Bearer $ARK_API_KEY
//! Models:   doubao-seed-2-0-pro-260215, doubao-seed-2-0-lite-260215, etc.

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, Message,
    ModelAdapter, ModelCapabilities, ModelError, ModelResponse, OptionAdjustment,
    ReasoningCapability, RequestOptions, Role, StopReason, StreamEvent, ThinkingLevel, ToolDef,
    UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

pub struct VolcengineAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
}

impl std::fmt::Debug for VolcengineAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolcengineAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct VolcengineConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl VolcengineAdapter {
    pub fn from_config(config: VolcengineConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::volcengine::API_KEY_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "ARK_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .unwrap_or_else(|| defaults::volcengine::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "Volcengine API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config
                .model
                .strip_prefix("volcengine/")
                .unwrap_or(&config.model)
                .to_string(),
            max_tokens: config.max_tokens,
        })
    }

    fn supports_thinking(&self) -> bool {
        self.model.starts_with("doubao-seed")
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

        if thinking_enabled {
            body["thinking"] = json!({"type": "enabled"});
        } else {
            body["thinking"] = json!({"type": "disabled"});
        }

        if options.thinking_budget_tokens.is_some() {
            adjustments.push(OptionAdjustment {
                option: "thinking_budget_tokens".into(),
                requested: json!(options.thinking_budget_tokens),
                applied: json!(null),
                reason: "unsupported_by_provider".into(),
            });
        }

        if options.cache_policy != CachePolicy::Auto && options.cache_policy != CachePolicy::None {
            adjustments.push(OptionAdjustment {
                option: "cache_policy".into(),
                requested: json!(format!("{:?}", options.cache_policy)),
                applied: json!("Auto"),
                reason: "volcengine_cache_automatic".into(),
            });
        }

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
    } else if trimmed.ends_with("/v3") {
        format!("{trimmed}/chat/completions")
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v3/chat/completions")
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

#[async_trait]
impl ModelAdapter for VolcengineAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let thinks = self.supports_thinking();
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: thinks,
                efforts: if thinks {
                    vec![ThinkingLevel::High, ThinkingLevel::Max]
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
            pricing: Some(crate::pricing::volcengine_pricing(&self.model)),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("volcengine", &self.model, tx.is_some());
        let thinking_enabled = options.thinking != ThinkingLevel::Off && self.supports_thinking();

        let effective_thinking = if !options.include_thinking && thinking_enabled {
            match options.compatibility_policy {
                CompatibilityPolicy::Strict => {
                    return Err(ModelError {
                        message: "Volcengine does not support output exclusion for reasoning"
                            .into(),
                        code: Some("unsupported_reasoning_output_exclusion".into()),
                        provider: Some("volcengine".into()),
                        status: None,
                        retry_after_secs: None,
                        upstream: None,
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
                applied: json!(false),
                reason: "thinking_disabled_for_output_exclusion".into(),
            });
        }

        let client = crate::http::shared_client();
        let start = Instant::now();

        let response = client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|err| ModelError::network(err.to_string(), "volcengine"))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let upstream_body = serde_json::from_str::<Value>(&text).ok();
            let upstream = upstream_body.as_ref().map(|body| UpstreamErrorDetail {
                code: body
                    .pointer("/error/code")
                    .or_else(|| body.get("code"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                message: body
                    .pointer("/error/message")
                    .or_else(|| body.get("message"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                body: upstream_body.clone(),
            });
            return Err(ModelError {
                message: format!("Volcengine API returned HTTP {status}"),
                code: Some("provider_http_error".into()),
                provider: Some("volcengine".into()),
                status: Some(status.as_u16()),
                retry_after_secs: None,
                upstream,
            });
        }

        let mut content = Vec::<ContentBlock>::new();
        let mut usage = crate::types::TokenUsage::default();
        let mut stop_reason = StopReason::EndTurn;
        let mut thinking_buf: Option<String> = None;
        let mut text_buf = String::new();

        let stream = crate::sse::SseStream::from_response(response);
        let stream = Arc::new(tokio::sync::Mutex::new(stream));

        loop {
            let line = {
                let mut s = stream.lock().await;
                s.next_line().await
            };

            match line {
                None => break,
                Some(Err(err)) => {
                    return Err(ModelError::network(err.to_string(), "volcengine"));
                }
                Some(Ok(data)) => {
                    if data == "[DONE]" {
                        break;
                    }
                    let chunk: Value = match serde_json::from_str(&data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    if let Some(arr) = chunk["choices"].as_array() {
                        for choice in arr {
                            let delta = &choice["delta"];

                            // reasoning_content → Thinking block
                            if let Some(rc) = delta
                                .get("reasoning_content")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                            {
                                let buf = thinking_buf.get_or_insert_with(String::new);
                                buf.push_str(rc);
                                if let Some(ref tx) = tx {
                                    let _ = tx
                                        .send(StreamEvent::Thinking {
                                            delta: rc.to_string(),
                                        })
                                        .await;
                                }
                            }

                            // content → Text
                            if let Some(text) = delta
                                .get("content")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                            {
                                text_buf.push_str(text);
                                if let Some(ref tx) = tx {
                                    let _ = tx
                                        .send(StreamEvent::Text {
                                            delta: text.to_string(),
                                        })
                                        .await;
                                }
                            }

                            // tool_calls
                            if let Some(tcs) = delta["tool_calls"].as_array() {
                                for tc in tcs {
                                    if let (Some(id), Some(name)) = (
                                        tc["id"].as_str(),
                                        tc["function"]["name"].as_str(),
                                    ) {
                                        let input: Value = tc["function"]["arguments"]
                                            .as_str()
                                            .and_then(|s| serde_json::from_str(s).ok())
                                            .unwrap_or(json!({}));
                                        content.push(ContentBlock::ToolUse {
                                            id: id.to_string(),
                                            name: name.to_string(),
                                            input,
                                        });
                                    }
                                }
                            }

                            if let Some(reason) = choice["finish_reason"].as_str() {
                                stop_reason = map_stop_reason(reason);
                            }
                        }
                    }

                    // usage
                    if let Some(u) = chunk.get("usage") {
                        usage.input_tokens =
                            u["prompt_tokens"].as_u64().unwrap_or(0) as u32;
                        usage.output_tokens =
                            u["completion_tokens"].as_u64().unwrap_or(0) as u32;
                    }
                }
            }
        }

        // Flush text/thinking buffers into content
        if let Some(thinking) = thinking_buf {
            if !thinking.is_empty() && options.include_thinking {
                content.insert(
                    0,
                    ContentBlock::Thinking {
                        text: Some(thinking),
                        encrypted: None,
                    },
                );
            }
        }
        if !text_buf.is_empty() {
            content.push(ContentBlock::Text(text_buf));
        }

        let latency_ms = start.elapsed().as_millis() as u64;
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        telemetry::record_model_latency("volcengine", &self.model, latency_ms);

        Ok(ModelResponse {
            content,
            usage,
            stop_reason,
            option_adjustments,
        })
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct VolcengineFactory;

impl crate::registry::ProviderFactory for VolcengineFactory {
    fn provider_name(&self) -> &'static str {
        "volcengine"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        Ok(Box::new(VolcengineAdapter::from_config(VolcengineConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?))
    }

    fn default_api_key_env(&self) -> &'static str {
        defaults::volcengine::API_KEY_ENV
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
