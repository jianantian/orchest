//! OpenAI adapter implementation.

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, Message,
    ModelAdapter, ModelCapabilities, ModelError, ModelResponse, OptionAdjustment,
    ReasoningCapability, RequestOptions, Role, StreamEvent, ThinkingLevel, ToolDef,
    UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

pub struct OpenAiAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
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
    let name = model.split_once('/').map_or(model, |(_, m)| m);
    name.starts_with("o1")
        || name.starts_with("o3")
        || name.starts_with("o4")
        || name.starts_with("gpt-5")
}

fn openai_context_window(model: &str) -> u64 {
    let name = model.split_once('/').map_or(model, |(_, m)| m);
    if name.starts_with("gpt-5.5") || name == "gpt-5.4" {
        1_000_000
    } else if name.starts_with("gpt-5.4-mini") || name.starts_with("gpt-5.4-nano") {
        400_000
    } else {
        128_000
    }
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
            context_window_size: Some(openai_context_window(&self.model)),
            source: CapabilitySource::Static,
            pricing: Some(crate::pricing::openai_pricing(&self.model)),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("openai", &self.model, tx.is_some());

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
        let response = crate::http::shared_client()
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

            telemetry::record_model_error("openai", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("openai".into()),
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

// ProviderFactory implementation

pub struct OpenAiFactory;

impl crate::registry::ProviderFactory for OpenAiFactory {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        crate::defaults::openai::API_KEY_ENV
    }
}

#[cfg(test)]
mod tests;
