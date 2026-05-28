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
        m.starts_with("claude-opus-4")
            || m.starts_with("claude-sonnet-4")
            || m.starts_with("claude-haiku-4")
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
            context_window_size: Some(200_000),
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
pub(crate) mod test_util {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub async fn serve_sse_once(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 8192];
            let _ = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
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

        format!("http://{address}")
    }

    pub async fn serve_sse_once_capture(
        body: &'static str,
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
            let mut request = vec![0; 16384];
            let n = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            let req_str = String::from_utf8_lossy(&request[..n]).to_string();
            let _ = capture_tx.send(req_str);

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

    pub async fn serve_status(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 8192];
            let _ = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            let response = format!(
                "HTTP/1.1 {status} Error\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("test server should write response");
        });

        format!("http://{address}")
    }

    pub async fn serve_partial_sse(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 8192];
            let _ = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            // No content-length, just write and drop the connection
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("test server should write response");
        });

        format!("http://{address}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::test_util::*;

    const MINIMAL_SSE: &str = r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#;

    fn default_options() -> RequestOptions {
        RequestOptions {
            thinking: ThinkingLevel::Off,
            ..Default::default()
        }
    }

    fn make_adapter(api_url: &str) -> AnthropicAdapter {
        AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url.into()),
        })
        .expect("adapter should be created")
    }

    #[test]
    fn uses_default_api_url() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: None,
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, defaults::anthropic::API_URL);
    }

    #[test]
    fn uses_custom_api_url() {
        let api_url = "https://compatible.example.com/v1/messages";
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url.into()),
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, api_url);
    }

    #[test]
    fn appends_messages_endpoint() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("https://openrouter.ai/api".into()),
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, "https://openrouter.ai/api/v1/messages");
    }

    #[test]
    fn rejects_empty_api_url() {
        let result = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(" ".into()),
        });

        let error = result.expect_err("empty api url should be rejected");
        assert_eq!(error.code.as_deref(), Some("invalid_api_url"));
    }

    #[tokio::test]
    async fn stream_thinking_boundaries() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":3}}}

event: content_block_start
data: {"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"first "}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"second"}}

event: content_block_stop
data: {"content_block":{"signature":"sig-abc-123"}}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

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

        assert!(matches!(events[0], StreamEvent::ThinkingStart));
        assert!(matches!(&events[1], StreamEvent::Thinking { delta } if delta == "first "));
        assert!(matches!(&events[2], StreamEvent::Thinking { delta } if delta == "second"));
        assert!(
            matches!(&events[3], StreamEvent::ThinkingEnd { ref signature, .. } if signature.as_deref() == Some("sig-abc-123"))
        );
        assert!(matches!(events[4], StreamEvent::Done { .. }));
        assert_eq!(response.usage.input_tokens, 3);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[tokio::test]
    async fn stream_rejects_malformed_sse() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":3}}}

event: content_block_delta
data: {NOT VALID JSON!!!}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let err = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect_err("malformed JSON should produce an error");

        assert_eq!(err.code.as_deref(), Some("invalid_json"));
        assert!(err.message.contains("malformed SSE JSON"));
        assert!(err.upstream.is_some());
    }

    #[tokio::test]
    async fn stream_thinking_to_content_block() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"reasoning here"}}

event: content_block_stop
data: {"content_block":{"signature":"my-sig"}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"answer"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");

        assert_eq!(response.content.len(), 2);
        match &response.content[0] {
            ContentBlock::Thinking {
                text,
                signature,
                provider_details,
            } => {
                assert_eq!(text.as_deref(), Some("reasoning here"));
                assert_eq!(signature.as_deref(), Some("my-sig"));
                assert!(provider_details.is_none());
            }
            _ => panic!("expected Thinking block"),
        }
        assert!(matches!(&response.content[1], ContentBlock::Text(t) if t == "answer"));
    }

    #[test]
    fn thinking_level_maps_to_budget() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-3-opus".into(), // old model, uses enabled mode
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
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32768);
    }

    #[test]
    fn thinking_budget_override() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-3-opus".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            thinking_budget_tokens: Some(8000),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["budget_tokens"], 8000);
    }

    #[test]
    fn thinking_budget_ignored_in_adaptive_reports_adjustment() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(), // adaptive model
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            thinking_budget_tokens: Some(8000),
            ..Default::default()
        };
        let (body, adjustments) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "high");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].reason, "unsupported_in_adaptive_thinking");
    }

    #[test]
    fn adaptive_uses_output_config_effort() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        for (level, expected) in [
            (ThinkingLevel::Low, "low"),
            (ThinkingLevel::Medium, "medium"),
            (ThinkingLevel::High, "high"),
            (ThinkingLevel::XHigh, "xhigh"),
            (ThinkingLevel::Max, "max"),
        ] {
            let opts = RequestOptions {
                thinking: level,
                ..Default::default()
            };
            let (body, _) = adapter.build_request_body(&[], &[], &opts);
            assert_eq!(body["thinking"]["type"], "adaptive");
            assert_eq!(body["output_config"]["effort"], expected, "level {level:?}");
        }
    }

    #[test]
    fn include_thinking_false_maps_to_omitted() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            include_thinking: false,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["display"], "omitted");
    }

    #[test]
    fn cache_policy_auto_adds_top_level_cache_control() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            cache_policy: CachePolicy::Auto,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn cache_policy_long_sets_1h_ttl() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            cache_policy: CachePolicy::Long,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["cache_control"]["ttl"], "1h");
    }

    #[tokio::test]
    async fn cache_usage_mapped_to_token_usage() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":100,"cache_read_input_tokens":50,"cache_creation_input_tokens":20}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");

        assert_eq!(response.usage.input_tokens, 100);
        assert_eq!(response.usage.cache_read_tokens, 50);
        assert_eq!(response.usage.cache_write_tokens, 20);
    }

    #[test]
    fn temperature_forwarded() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            temperature: Some(0.7),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert!(
            body["temperature"].as_f64().unwrap() > 0.69
                && body["temperature"].as_f64().unwrap() < 0.71
        );
    }

    #[test]
    fn adaptive_vs_enabled_mode() {
        let adaptive = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();
        assert!(adaptive.supports_adaptive());

        let enabled = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-3-opus".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();
        assert!(!enabled.supports_adaptive());
    }

    #[tokio::test]
    async fn tool_use_start_and_end_emitted() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"tool_1","name":"read_file"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"/tmp\"}"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}

event: message_stop
data: {}

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

        assert!(
            matches!(&events[0], StreamEvent::ToolUseStart { id, name } if id == "tool_1" && name == "read_file")
        );
        assert!(matches!(&events[1], StreamEvent::ToolUseArgsChunk { id, .. } if id == "tool_1"));
        assert!(matches!(&events[2], StreamEvent::ToolUseEnd { id } if id == "tool_1"));
        assert!(matches!(events[3], StreamEvent::Done { .. }));
        assert_eq!(response.stop_reason, StopReason::ToolUse);
    }

    #[tokio::test]
    async fn parallel_tool_uses_end_each() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"t1","name":"a"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{}"}}

event: content_block_stop
data: {}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"t2","name":"b"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{}"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let (tx, mut rx) = mpsc::channel(32);
        adapter
            .complete(&[], &[], &default_options(), Some(tx))
            .await
            .expect("should parse");

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        let end_ids: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ToolUseEnd { id } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(end_ids, vec!["t1", "t2"]);
    }

    #[tokio::test]
    async fn tx_none_skips_events() {
        let api_url = serve_sse_once(MINIMAL_SSE).await;
        let adapter = make_adapter(&api_url);

        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");

        assert_eq!(response.content.len(), 1);
        assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "Hello"));
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn max_tokens_override() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            max_tokens: Some(4096),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["max_tokens"], 4096);

        let opts_none = RequestOptions {
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            ..Default::default()
        };
        let (body2, _) = adapter.build_request_body(&[], &[], &opts_none);
        assert_eq!(body2["max_tokens"], 128);
    }

    #[tokio::test]
    async fn stream_interrupted_returns_error() {
        let api_url = serve_partial_sse(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"partial"}}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let err = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect_err("interrupted stream should error");

        assert_eq!(err.code.as_deref(), Some("stream_interrupted"));
    }

    #[tokio::test]
    async fn missing_usage_reports_adjustment() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should succeed");

        let adj = response
            .option_adjustments
            .iter()
            .find(|a| a.reason == "usage_not_reported");
        assert!(adj.is_some(), "should report usage_not_reported adjustment");
    }

    #[tokio::test]
    async fn done_usage_matches_model_response() {
        let api_url = serve_sse_once(MINIMAL_SSE).await;
        let adapter = make_adapter(&api_url);

        let (tx, mut rx) = mpsc::channel(16);
        let response = adapter
            .complete(&[], &[], &default_options(), Some(tx))
            .await
            .expect("should parse");

        let mut done_usage = None;
        while let Some(e) = rx.recv().await {
            if let StreamEvent::Done { usage } = e {
                done_usage = Some(usage);
            }
        }

        let done_usage = done_usage.expect("should have Done event");
        assert_eq!(done_usage.input_tokens, response.usage.input_tokens);
        assert_eq!(done_usage.output_tokens, response.usage.output_tokens);
        assert!(
            response.usage.cost_usd.is_some(),
            "response should have cost_usd filled by adapter"
        );
    }

    #[test]
    fn provider_name_and_model_name() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        assert_eq!(adapter.provider_name(), "anthropic");
        assert_eq!(adapter.model_name(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn capabilities_reports_static_source() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let caps = adapter.capabilities();
        assert!(caps.streaming);
        assert!(caps.tool_use);
        assert!(caps.reasoning.supported);
        assert!(caps.prompt_cache.supported);
        assert_eq!(caps.source, CapabilitySource::Static);
    }

    #[tokio::test]
    async fn stop_reason_mapping() {
        for (raw, expected) in [
            ("end_turn", StopReason::EndTurn),
            ("tool_use", StopReason::ToolUse),
            ("max_tokens", StopReason::MaxTokens),
            ("stop_sequence", StopReason::StopSequence),
            ("pause_turn", StopReason::Pause),
            ("compaction", StopReason::Pause),
            ("refusal", StopReason::Refusal),
            (
                "model_context_window_exceeded",
                StopReason::ContextWindowExceeded,
            ),
        ] {
            assert_eq!(map_stop_reason(raw), expected, "for {raw}");
        }
        assert!(
            matches!(map_stop_reason("unknown_reason"), StopReason::Other(s) if s == "unknown_reason")
        );
    }
}
