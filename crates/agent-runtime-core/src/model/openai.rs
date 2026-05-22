use std::env;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{
    ContentBlock, Message, ModelAdapter, ModelError, ModelResponse, ModelSpec, ModelStreamChunk,
    Role, StopReason, TokenUsage,
};
use crate::tool::ToolDef;

const DEFAULT_API_URL: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    client: reqwest::Client,
}

pub struct OpenAiConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl OpenAiAdapter {
    pub fn new(spec: ModelSpec, api_key: Option<String>) -> Result<Self, ModelError> {
        Self::from_config(OpenAiConfig {
            model: spec
                .model
                .strip_prefix("openai/")
                .unwrap_or(&spec.model)
                .to_string(),
            max_tokens: spec.max_tokens.unwrap_or(4096),
            api_key,
            api_url: spec.api_url,
        })
    }

    pub fn from_config(config: OpenAiConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| ModelError {
                message: "OPENAI_API_KEY not set and no api_key provided".into(),
                code: Some("missing_api_key".into()),
            })?;
        let api_url = config
            .api_url
            .or_else(|| env::var("OPENAI_API_URL").ok())
            .or_else(|| env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());
        if api_url.trim().is_empty() {
            return Err(ModelError {
                message: "OpenAI API URL cannot be empty".into(),
                code: Some("invalid_api_url".into()),
            });
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

    fn build_request_body(&self, messages: &[Message], tools: &[ToolDef]) -> Value {
        let mut api_messages: Vec<Value> = Vec::new();

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
                    // User messages may contain ToolResult blocks (runtime sends them as User role)
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
                        // Emit each tool result as a separate "tool" role message
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
                    let mut tool_calls = Vec::new();
                    for block in &message.content {
                        match block {
                            ContentBlock::Text(t) => text_parts.push(t.clone()),
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string()
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }
                    let mut msg = json!({"role": "assistant"});
                    if !text_parts.is_empty() {
                        msg["content"] = json!(text_parts.join("\n"));
                    } else if tool_calls.is_empty() {
                        msg["content"] = json!("");
                    }
                    if !tool_calls.is_empty() {
                        msg["tool_calls"] = Value::Array(tool_calls);
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

        let mut body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
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
        body
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

#[async_trait::async_trait]
impl ModelAdapter for OpenAiAdapter {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError> {
        let response = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&self.build_request_body(messages, tools))
            .send()
            .await
            .map_err(|e| ModelError {
                message: e.to_string(),
                code: Some("request_failed".into()),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError {
                message: format!("API returned {status}: {body}"),
                code: Some(status.as_u16().to_string()),
            });
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut text = String::new();
        let mut tool_calls: Vec<(String, String, String)> = Vec::new();
        let mut usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let mut stop_reason = StopReason::EndTurn;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ModelError {
                message: e.to_string(),
                code: Some("stream_error".into()),
            })?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find("\n\n") {
                let block = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();
                let data = block
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap_or_default();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let value: Value = serde_json::from_str(data).map_err(|e| ModelError {
                    message: e.to_string(),
                    code: Some("invalid_json".into()),
                })?;
                if let Some(usage_value) = value.get("usage") {
                    usage.input_tokens = usage_value
                        .get("prompt_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(usage.input_tokens);
                    usage.output_tokens = usage_value
                        .get("completion_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(usage.output_tokens);
                }
                let Some(choice) = value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|v| v.first())
                else {
                    continue;
                };
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                    stop_reason = if reason == "tool_calls" {
                        StopReason::ToolUse
                    } else {
                        StopReason::EndTurn
                    };
                }
                let Some(delta) = choice.get("delta") else {
                    continue;
                };
                if let Some(content) = delta.get("content").and_then(Value::as_str) {
                    text.push_str(content);
                    let _ = tx
                        .send(ModelStreamChunk::Text {
                            delta: content.to_string(),
                        })
                        .await;
                }
                if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        let idx = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        while tool_calls.len() <= idx {
                            tool_calls.push((String::new(), String::new(), String::new()));
                        }
                        if let Some(id) = call.get("id").and_then(Value::as_str) {
                            tool_calls[idx].0 = id.to_string();
                        }
                        if let Some(function) = call.get("function") {
                            if let Some(name) = function.get("name").and_then(Value::as_str) {
                                tool_calls[idx].1 = name.to_string();
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                tool_calls[idx].2.push_str(arguments);
                                let _ = tx
                                    .send(ModelStreamChunk::ToolCallArgsChunk {
                                        id: tool_calls[idx].0.clone(),
                                        delta: arguments.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                }
            }
        }

        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(ContentBlock::Text(text));
        }
        for (id, name, arguments) in tool_calls {
            if name.is_empty() {
                continue;
            }
            let input = match serde_json::from_str(&arguments) {
                Ok(v) => v,
                Err(_) => {
                    return Err(ModelError {
                        message: format!(
                            "invalid tool call arguments for '{}': {}",
                            name, arguments
                        ),
                        code: Some("invalid_tool_arguments".into()),
                    });
                }
            };
            content.push(ContentBlock::ToolUse { id, name, input });
        }
        let _ = tx
            .send(ModelStreamChunk::Done {
                usage: usage.clone(),
            })
            .await;
        Ok(ModelResponse {
            content,
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
    fn strips_openai_prefix_from_model_spec() {
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
    async fn stream_normalizes_text_tool_calls_and_usage() {
        let api_url = serve_sse_once(
            r#"data: {"choices":[{"delta":{"content":"hi "}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"{\"text\":"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hello\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":7,"completion_tokens":9}}

data: [DONE]

"#,
        )
        .await;
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: "gpt-4o-mini".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url),
        })
        .expect("adapter");
        let (tx, mut rx) = mpsc::channel(16);
        let response = adapter
            .stream(&[], &[], tx)
            .await
            .expect("stream should parse");
        let mut chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks.push(chunk);
        }
        assert!(matches!(
            &chunks[0],
            ModelStreamChunk::Text { delta } if delta == "hi "
        ));
        assert!(!chunks
            .iter()
            .any(|chunk| matches!(chunk, ModelStreamChunk::Thinking { .. })));
        assert_eq!(response.usage.input_tokens, 7);
        assert_eq!(response.usage.output_tokens, 9);
        assert!(matches!(response.stop_reason, StopReason::ToolUse));
        assert!(matches!(
            &response.content[1],
            ContentBlock::ToolUse { id, name, input }
                if id == "call_1" && name == "echo" && input["text"] == "hello"
        ));
    }

    #[test]
    fn build_request_body_serializes_tool_calls_and_results() {
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: "gpt-4o-mini".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost/v1/chat/completions".into()),
        })
        .expect("adapter");

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

        let body = adapter.build_request_body(&messages, &[]);
        let api_msgs = body["messages"].as_array().unwrap();

        // System message
        assert_eq!(api_msgs[0]["role"], "system");
        assert_eq!(api_msgs[0]["content"], "You are helpful.");

        // User message
        assert_eq!(api_msgs[1]["role"], "user");
        assert_eq!(api_msgs[1]["content"], "Hello");

        // Assistant message with tool_calls
        assert_eq!(api_msgs[2]["role"], "assistant");
        assert_eq!(api_msgs[2]["content"], "Let me check.");
        let tool_calls = api_msgs[2]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");

        // Tool result message
        assert_eq!(api_msgs[3]["role"], "tool");
        assert_eq!(api_msgs[3]["tool_call_id"], "call_1");
    }

    #[tokio::test]
    async fn stream_rejects_invalid_tool_arguments() {
        let api_url = serve_sse_once(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"not valid json"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}

data: [DONE]

"#,
        )
        .await;
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: "gpt-4o-mini".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url),
        })
        .expect("adapter");
        let (tx, _rx) = mpsc::channel(16);
        let result = adapter.stream(&[], &[], tx).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code.as_deref(), Some("invalid_tool_arguments"));
    }

    async fn serve_sse_once(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = vec![0; 4096];
            let _ = socket.read(&mut request).await.expect("read");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.expect("write");
        });
        format!("http://{address}/v1/chat/completions")
    }
}
