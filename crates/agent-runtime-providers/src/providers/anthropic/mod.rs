//! Anthropic Claude adapter implementation.

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, Message,
    ModelAdapter, ModelCapabilities, ModelError, ModelPricing, ModelResponse, OptionAdjustment,
    ReasoningCapability, RequestOptions, Role, StopReason, StreamEvent, ThinkingLevel, TokenUsage,
    ToolDef, UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

pub struct AnthropicAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
}

impl std::fmt::Debug for AnthropicAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct AnthropicConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl AnthropicAdapter {
    pub fn from_config(config: AnthropicConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::anthropic::API_KEY_ENV).ok())
            .or_else(|| env::var(defaults::anthropic::AUTH_TOKEN_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .or_else(|| {
                env::var(defaults::anthropic::API_URL_ENV)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| defaults::anthropic::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "Anthropic API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        Ok(Self {
            api_key,
            api_url: normalize_messages_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
        })
    }

    fn supports_adaptive(&self) -> bool {
        let m = &self.model;
        // fable-5 and mythos-5: adaptive always-on
        // opus-4.x and sonnet-4.x: adaptive always-on
        // haiku-4.x: extended thinking only (no adaptive)
        m.starts_with("claude-fable-5")
            || m.starts_with("claude-mythos-5")
            || m.starts_with("claude-opus-4")
            || m.starts_with("claude-sonnet-4")
    }

    fn context_window_size(&self) -> u64 {
        let m = &self.model;
        if m.starts_with("claude-haiku-4") {
            200_000
        } else {
            1_000_000
        }
    }

    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> (Value, Vec<OptionAdjustment>) {
        let mut system_parts = Vec::new();
        let mut api_messages = Vec::new();
        let mut adjustments = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    for block in &msg.content {
                        if let ContentBlock::Text(t) = block {
                            system_parts.push(t.clone());
                        }
                    }
                }
                _ => {
                    let role = match msg.role {
                        Role::User | Role::Tool => "user",
                        Role::Assistant => "assistant",
                        Role::System => unreachable!(),
                    };

                    let content: Vec<Value> = msg
                        .content
                        .iter()
                        .map(|block| match block {
                            ContentBlock::Text(t) => json!({"type": "text", "text": t}),
                            ContentBlock::Thinking { text, signature, .. } => {
                                let mut obj = json!({"type": "thinking"});
                                if let Some(t) = text {
                                    obj["thinking"] = json!(t);
                                }
                                if let Some(s) = signature {
                                    obj["signature"] = json!(s);
                                }
                                obj
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                json!({"type": "tool_use", "id": id, "name": name, "input": input})
                            }
                            ContentBlock::ToolResult { tool_use_id, content } => {
                                json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content})
                            }
                        })
                        .collect();

                    api_messages.push(json!({"role": role, "content": content}));
                }
            }
        }

        let effective_max_tokens = options.max_tokens.unwrap_or(self.max_tokens);

        let mut body = json!({
            "model": self.model,
            "max_tokens": effective_max_tokens,
            "messages": api_messages,
            "stream": true,
        });

        if !system_parts.is_empty() {
            body["system"] = json!(system_parts.join("\n\n"));
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = json!(tool_defs);
        }

        // ThinkingLevel mapping
        match options.thinking {
            ThinkingLevel::Off => {
                body["thinking"] = json!({"type": "disabled"});
            }
            level => {
                if self.supports_adaptive() {
                    let effort = match level {
                        ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                        ThinkingLevel::Medium => "medium",
                        ThinkingLevel::High => "high",
                        ThinkingLevel::XHigh => "xhigh",
                        ThinkingLevel::Max => "max",
                        ThinkingLevel::Off => unreachable!(),
                    };
                    body["thinking"] = json!({"type": "adaptive"});

                    if options.include_thinking {
                        body["thinking"]["display"] = json!("summarized");
                    } else {
                        body["thinking"]["display"] = json!("omitted");
                    }

                    body["output_config"] = json!({"effort": effort});

                    if options.thinking_budget_tokens.is_some() {
                        adjustments.push(OptionAdjustment {
                            option: "thinking_budget_tokens".into(),
                            requested: json!(options.thinking_budget_tokens),
                            applied: json!(null),
                            reason: "unsupported_in_adaptive_thinking".into(),
                        });
                    }
                } else {
                    let budget = options.thinking_budget_tokens.unwrap_or(match level {
                        ThinkingLevel::Minimal => 1024,
                        ThinkingLevel::Low => 4096,
                        ThinkingLevel::Medium => 10240,
                        ThinkingLevel::High => 32768,
                        ThinkingLevel::XHigh => 65536,
                        ThinkingLevel::Max => effective_max_tokens,
                        ThinkingLevel::Off => unreachable!(),
                    });
                    body["thinking"] = json!({
                        "type": "enabled",
                        "budget_tokens": budget,
                    });

                    if options.include_thinking {
                        body["thinking"]["display"] = json!("summarized");
                    } else {
                        body["thinking"]["display"] = json!("omitted");
                    }
                }
            }
        }

        // CachePolicy mapping
        match options.cache_policy {
            CachePolicy::Auto => {
                body["cache_control"] = json!({"type": "ephemeral"});
            }
            CachePolicy::Long => {
                body["cache_control"] = json!({"type": "ephemeral", "ttl": "1h"});
            }
            CachePolicy::None => {}
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

fn normalize_messages_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1/messages") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1/messages")
    }
}

fn map_stop_reason(raw: &str) -> StopReason {
    match raw {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "pause_turn" | "compaction" => StopReason::Pause,
        "refusal" => StopReason::Refusal,
        "model_context_window_exceeded" => StopReason::ContextWindowExceeded,
        other => StopReason::Other(other.to_string()),
    }
}

impl AnthropicAdapter {
    fn pricing(&self) -> ModelPricing {
        crate::pricing::anthropic_pricing(&self.model)
    }
}

#[async_trait]
#[allow(clippy::too_many_lines)] // justified: streaming SSE + tool-use mapping in one impl block, splitting would fragment cohesive logic
impl ModelAdapter for AnthropicAdapter {
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let supports_adaptive = self.supports_adaptive();
        let efforts = if supports_adaptive {
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Minimal,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
                ThinkingLevel::Max,
            ]
        } else {
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ]
        };

        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts,
                budget_tokens: !supports_adaptive,
                output_exclusion: true,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: true,
                long_ttl: true,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: Some(self.context_window_size()),
            source: CapabilitySource::Static,
            pricing: Some(self.pricing()),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("anthropic", &self.model, tx.is_some());

        let (body, mut option_adjustments) = self.build_request_body(messages, tools, options);

        if options.compatibility_policy == CompatibilityPolicy::Strict
            && options.thinking_budget_tokens.is_some()
            && self.supports_adaptive()
            && options.thinking != ThinkingLevel::Off
        {
            return Err(ModelError::internal(
                "thinking_budget_tokens is not supported in adaptive thinking mode",
                "unsupported_thinking_budget",
            ));
        }

        let start = Instant::now();
        let response = crate::http::shared_client()
            .post(&self.api_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", defaults::anthropic::API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("anthropic", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("anthropic".into()),
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

            telemetry::record_model_error("anthropic", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("anthropic".into()),
                status: Some(status),
                retry_after_secs: None,
                upstream: Some(Arc::new(UpstreamErrorDetail {
                    code: upstream_code,
                    message: upstream_msg,
                    body: upstream_body,
                })),
            });
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_block_type: Option<String> = None;
        let mut current_block_id: Option<String> = None;
        let mut current_block_name: Option<String> = None;
        let mut current_text = String::new();
        let mut current_thinking_text = String::new();
        let mut current_tool_input_json = String::new();
        let mut usage = TokenUsage::default();
        let mut stop_reason = StopReason::EndTurn;
        let mut got_message_stop = false;
        let mut first_token_latency: Option<std::time::Duration> = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| ModelError {
                message: e.to_string(),
                code: Some("stream_error".into()),
                provider: Some("anthropic".into()),
                status: None,
                retry_after_secs: None,
                upstream: None,
            })?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let event_block = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                let mut event_type = String::new();
                let mut event_data = String::new();

                for line in event_block.lines() {
                    if let Some(et) = line.strip_prefix("event: ") {
                        event_type = et.to_string();
                    } else if let Some(ed) = line.strip_prefix("data: ") {
                        event_data = ed.to_string();
                    }
                }

                if event_data.is_empty() {
                    continue;
                }

                let data: Value = serde_json::from_str(&event_data).map_err(|e| ModelError {
                    message: format!("malformed SSE JSON: {e}"),
                    code: Some("invalid_json".into()),
                    provider: Some("anthropic".into()),
                    status: None,
                    retry_after_secs: None,
                    upstream: Some(Arc::new(UpstreamErrorDetail {
                        code: None,
                        message: None,
                        body: Some(json!(event_data)),
                    })),
                })?;

                match event_type.as_str() {
                    "content_block_start" => {
                        if let Some(block) = data.get("content_block") {
                            current_block_type =
                                block.get("type").and_then(|v| v.as_str()).map(String::from);
                            current_block_id =
                                block.get("id").and_then(|v| v.as_str()).map(String::from);
                            current_block_name =
                                block.get("name").and_then(|v| v.as_str()).map(String::from);
                            current_text.clear();
                            current_thinking_text.clear();
                            current_tool_input_json.clear();

                            match current_block_type.as_deref() {
                                Some("thinking") => {
                                    if let Some(ref tx) = tx {
                                        let _ = tx.send(StreamEvent::ThinkingStart).await;
                                    }
                                }
                                Some("tool_use") => {
                                    if let Some(ref tx) = tx {
                                        let _ = tx
                                            .send(StreamEvent::ToolUseStart {
                                                id: current_block_id.clone().unwrap_or_default(),
                                                name: current_block_name
                                                    .clone()
                                                    .unwrap_or_default(),
                                            })
                                            .await;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Some(delta) = data.get("delta") {
                            let delta_type =
                                delta.get("type").and_then(|v| v.as_str()).unwrap_or("");

                            match delta_type {
                                "text_delta" => {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        first_token_latency.get_or_insert_with(|| start.elapsed());
                                        current_text.push_str(text);
                                        if let Some(ref tx) = tx {
                                            let _ = tx
                                                .send(StreamEvent::Text {
                                                    delta: text.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(partial) =
                                        delta.get("partial_json").and_then(|v| v.as_str())
                                    {
                                        first_token_latency.get_or_insert_with(|| start.elapsed());
                                        current_tool_input_json.push_str(partial);
                                        if let Some(ref tx) = tx {
                                            if let Some(id) = &current_block_id {
                                                let _ = tx
                                                    .send(StreamEvent::ToolUseArgsChunk {
                                                        id: id.clone(),
                                                        delta: partial.to_string(),
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                "thinking_delta" => {
                                    if let Some(text) =
                                        delta.get("thinking").and_then(|v| v.as_str())
                                    {
                                        current_thinking_text.push_str(text);
                                        if let Some(ref tx) = tx {
                                            let _ = tx
                                                .send(StreamEvent::Thinking {
                                                    delta: text.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_stop" => {
                        match current_block_type.as_deref() {
                            Some("text") => {
                                content_blocks.push(ContentBlock::Text(current_text.clone()));
                            }
                            Some("tool_use") => {
                                let input: Value = serde_json::from_str(&current_tool_input_json)
                                    .unwrap_or(Value::Object(Default::default()));
                                let id = current_block_id.clone().unwrap_or_default();
                                content_blocks.push(ContentBlock::ToolUse {
                                    id: id.clone(),
                                    name: current_block_name.clone().unwrap_or_default(),
                                    input,
                                });
                                if let Some(ref tx) = tx {
                                    let _ = tx.send(StreamEvent::ToolUseEnd { id }).await;
                                }
                            }
                            Some("thinking") => {
                                let signature = data
                                    .get("content_block")
                                    .and_then(|b| b.get("signature"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);

                                let thinking_text = if current_thinking_text.is_empty() {
                                    None
                                } else {
                                    Some(current_thinking_text.clone())
                                };

                                content_blocks.push(ContentBlock::Thinking {
                                    text: thinking_text,
                                    signature: signature.clone(),
                                    provider_details: None,
                                });

                                if let Some(ref tx) = tx {
                                    let _ = tx
                                        .send(StreamEvent::ThinkingEnd {
                                            signature,
                                            provider_details: None,
                                        })
                                        .await;
                                }
                            }
                            _ => {}
                        }
                        current_block_type = None;
                        current_block_id = None;
                        current_block_name = None;
                    }
                    "message_delta" => {
                        if let Some(delta) = data.get("delta") {
                            if let Some(sr) = delta.get("stop_reason").and_then(|v| v.as_str()) {
                                stop_reason = map_stop_reason(sr);
                            }
                        }
                        if let Some(u) = data.get("usage") {
                            if let Some(ot) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                                usage.output_tokens = ot;
                            }
                        }
                    }
                    "message_start" => {
                        if let Some(msg) = data.get("message") {
                            if let Some(u) = msg.get("usage") {
                                if let Some(it) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                                    usage.input_tokens = it;
                                }
                                if let Some(cr) =
                                    u.get("cache_read_input_tokens").and_then(|v| v.as_u64())
                                {
                                    usage.cache_read_tokens = cr;
                                }
                                if let Some(cw) = u
                                    .get("cache_creation_input_tokens")
                                    .and_then(|v| v.as_u64())
                                {
                                    usage.cache_write_tokens = cw;
                                }
                            }
                        }
                    }
                    "message_stop" => {
                        got_message_stop = true;
                    }
                    _ => {}
                }
            }
        }

        if !got_message_stop {
            return Err(ModelError {
                message: "SSE stream ended without message_stop".into(),
                code: Some("stream_interrupted".into()),
                provider: Some("anthropic".into()),
                status: None,
                retry_after_secs: None,
                upstream: None,
            });
        }

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("anthropic", &self.model);
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
            "anthropic",
            &self.model,
            duration,
            usage.input_tokens,
            usage.output_tokens,
            first_token_latency,
            Some(duration),
        );

        usage.cost_usd = Some(self.pricing().calculate(&usage));

        Ok(ModelResponse {
            content: content_blocks,
            usage,
            stop_reason,
            option_adjustments,
        })
    }
}

// ProviderFactory implementation

pub struct AnthropicFactory;

impl crate::registry::ProviderFactory for AnthropicFactory {
    fn provider_name(&self) -> &'static str {
        "anthropic"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        defaults::anthropic::API_KEY_ENV
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod test_util;

#[cfg(test)]
mod tests;
