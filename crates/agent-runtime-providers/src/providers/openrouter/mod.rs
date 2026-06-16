//! OpenRouter adapter implementation (multi-provider routing).

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CapabilitySource, ContentBlock, Message, ModelAdapter, ModelCapabilities,
    ModelError, ModelResponse, OptionAdjustment, ReasoningCapability, RequestOptions, Role,
    StreamEvent, ThinkingLevel, ToolDef, UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

pub struct OpenRouterAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    app_title: Option<String>,
    site_url: Option<String>,
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
            retry_after_secs: None,
            upstream: Some(Arc::new(UpstreamErrorDetail {
                code: None,
                message: None,
                body: Some(other.clone()),
            })),
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
            pricing: None,
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

        let mut request = crate::http::shared_client()
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
                retry_after_secs: None,
                upstream: None,
            }
        })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_text = response.text().await.unwrap_or_default();
            let upstream_body: Option<Value> = serde_json::from_str(&body_text).ok();
            let (upstream_code, upstream_msg) = upstream_body
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
                retry_after_secs: None,
                upstream: Some(Arc::new(UpstreamErrorDetail {
                    code: upstream_code,
                    message: upstream_msg,
                    body: upstream_body,
                })),
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

// ProviderFactory implementation

pub struct OpenRouterFactory;

impl crate::registry::ProviderFactory for OpenRouterFactory {
    fn provider_name(&self) -> &'static str {
        "openrouter"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
            app_title: None,
            site_url: None,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        crate::defaults::openrouter::API_KEY_ENV
    }
}

#[cfg(test)]
mod tests;
