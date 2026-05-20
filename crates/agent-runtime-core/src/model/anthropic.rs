use std::env;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{
    ContentBlock, Message, ModelAdapter, ModelError, ModelResponse, ModelStreamChunk, Role,
    StopReason, TokenUsage,
};
use crate::tool::ToolDef;

const DEFAULT_API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    client: reqwest::Client,
}

pub struct AnthropicConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl AnthropicAdapter {
    pub fn new(
        model: String,
        max_tokens: u32,
        api_key: Option<String>,
    ) -> Result<Self, ModelError> {
        Self::new_with_api_url(
            model,
            max_tokens,
            api_key,
            env::var("ANTHROPIC_API_URL").ok(),
        )
    }

    pub fn new_with_api_url(
        model: String,
        max_tokens: u32,
        api_key: Option<String>,
        api_url: Option<String>,
    ) -> Result<Self, ModelError> {
        Self::from_config(AnthropicConfig {
            model,
            max_tokens,
            api_key,
            api_url,
        })
    }

    pub fn from_config(config: AnthropicConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var("ANTHROPIC_API_KEY").ok())
            .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok())
            .or_else(|| env::var("OPENROUTER_API_KEY").ok())
            .ok_or_else(|| ModelError {
                message:
                    "ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN, OPENROUTER_API_KEY not set and no api_key provided"
                        .into(),
                code: Some("missing_api_key".into()),
            })?;
        let api_url = config
            .api_url
            .or_else(|| env::var("ANTHROPIC_API_URL").ok())
            .or_else(|| env::var("ANTHROPIC_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError {
                message: "Anthropic API URL cannot be empty".into(),
                code: Some("invalid_api_url".into()),
            });
        }

        Ok(Self {
            api_key,
            api_url: normalize_messages_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
            client: reqwest::Client::new(),
        })
    }

    fn build_request_body(&self, messages: &[Message], tools: &[ToolDef]) -> Value {
        let mut system_parts = Vec::new();
        let mut api_messages = Vec::new();

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
                            ContentBlock::ToolUse { id, name, input } => {
                                json!({"type": "tool_use", "id": id, "name": name, "input": input})
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                            } => {
                                json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content})
                            }
                        })
                        .collect();

                    api_messages.push(json!({"role": role, "content": content}));
                }
            }
        }

        let mut body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
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

        body
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

#[async_trait::async_trait]
impl ModelAdapter for AnthropicAdapter {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError> {
        let body = self.build_request_body(messages, tools);

        let response = self
            .client
            .post(&self.api_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ModelError {
                message: e.to_string(),
                code: Some("request_failed".into()),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.as_u16().to_string()),
            });
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_block_type: Option<String> = None;
        let mut current_block_id: Option<String> = None;
        let mut current_block_name: Option<String> = None;
        let mut current_text = String::new();
        let mut current_tool_input_json = String::new();
        let mut usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let mut stop_reason = StopReason::EndTurn;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| ModelError {
                message: e.to_string(),
                code: Some("stream_error".into()),
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

                let data: Value = serde_json::from_str(&event_data).unwrap_or_default();

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
                            current_tool_input_json.clear();

                            if current_block_type.as_deref() == Some("thinking") {
                                let _ = tx.send(ModelStreamChunk::ThinkingStart).await;
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
                                        current_text.push_str(text);
                                        let _ = tx
                                            .send(ModelStreamChunk::Text {
                                                delta: text.to_string(),
                                            })
                                            .await;
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(partial) =
                                        delta.get("partial_json").and_then(|v| v.as_str())
                                    {
                                        current_tool_input_json.push_str(partial);
                                        if let Some(id) = &current_block_id {
                                            let _ = tx
                                                .send(ModelStreamChunk::ToolCallArgsChunk {
                                                    id: id.clone(),
                                                    delta: partial.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                                "thinking_delta" => {
                                    if let Some(text) =
                                        delta.get("thinking").and_then(|v| v.as_str())
                                    {
                                        let _ = tx
                                            .send(ModelStreamChunk::Thinking {
                                                delta: text.to_string(),
                                            })
                                            .await;
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
                                content_blocks.push(ContentBlock::ToolUse {
                                    id: current_block_id.clone().unwrap_or_default(),
                                    name: current_block_name.clone().unwrap_or_default(),
                                    input,
                                });
                            }
                            Some("thinking") => {
                                let _ = tx.send(ModelStreamChunk::ThinkingEnd).await;
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
                                stop_reason = match sr {
                                    "tool_use" => StopReason::ToolUse,
                                    "max_tokens" => StopReason::MaxTokens,
                                    _ => StopReason::EndTurn,
                                };
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
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let _ = tx
            .send(ModelStreamChunk::Done {
                usage: usage.clone(),
            })
            .await;

        Ok(ModelResponse {
            content: content_blocks,
            usage,
            stop_reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn uses_default_api_url_when_none_is_provided() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: None,
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, DEFAULT_API_URL);
    }

    #[test]
    fn uses_custom_api_url_when_provided() {
        let api_url = "https://compatible.example.com/v1/messages".to_string();
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url.clone()),
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, api_url);
    }

    #[test]
    fn appends_messages_endpoint_to_base_url() {
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

        let error = match result {
            Ok(_) => panic!("empty api url should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.code.as_deref(), Some("invalid_api_url"));
    }

    #[tokio::test]
    async fn stream_emits_thinking_boundaries_from_provider_events() {
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
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url),
        })
        .expect("adapter should be created");

        let (tx, mut rx) = mpsc::channel(16);
        let response = adapter
            .stream(&[], &[], tx)
            .await
            .expect("stream should parse provider response");

        let mut chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks.push(chunk);
        }

        assert!(matches!(chunks[0], ModelStreamChunk::ThinkingStart));
        assert!(matches!(
            &chunks[1],
            ModelStreamChunk::Thinking { delta } if delta == "first "
        ));
        assert!(matches!(
            &chunks[2],
            ModelStreamChunk::Thinking { delta } if delta == "second"
        ));
        assert!(matches!(chunks[3], ModelStreamChunk::ThinkingEnd));
        assert!(matches!(chunks[4], ModelStreamChunk::Done { .. }));
        assert_eq!(response.usage.input_tokens, 3);
        assert_eq!(response.usage.output_tokens, 5);
    }

    async fn serve_sse_once(body: &'static str) -> String {
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
            let mut request = vec![0; 4096];
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

        format!("http://{address}/v1/messages")
    }
}
