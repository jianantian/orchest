//! Server-Sent Events (SSE) stream parser for OpenAI-compatible APIs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{ContentBlock, ModelError, StopReason, StreamEvent, TokenUsage, UpstreamErrorDetail};

struct ToolCallAccum {
    id: String,
    name: String,
    args: String,
    started: bool,
}

fn map_stop_reason(raw: &str) -> StopReason {
    match raw {
        "stop" => StopReason::EndTurn,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        "content_filter" => StopReason::ContentFilter,
        other => StopReason::Other(other.to_string()),
    }
}

#[derive(Debug)]
pub(crate) struct SseParseResult {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    pub first_token_latency: Option<Duration>,
}

pub(crate) async fn parse_openai_sse_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>>,
    tx: Option<&mpsc::Sender<StreamEvent>>,
    reasoning_field: Option<&str>,
    reasoning_details_field: Option<&str>,
    start: Instant,
) -> Result<SseParseResult, ModelError> {
    tokio::pin!(stream);

    let mut buffer = String::new();
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCallAccum> = Vec::new();
    let mut usage = TokenUsage::default();
    let mut stop_reason = StopReason::EndTurn;
    let mut got_done = false;
    let mut thinking_text = String::new();
    let mut thinking_active = false;
    let mut thinking_details: Option<Value> = None;
    let mut first_token_latency: Option<Duration> = None;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| ModelError {
            message: e.to_string(),
            code: Some("stream_error".into()),
            provider: None,
            status: None,
            upstream: None,
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
                if data == "[DONE]" {
                    got_done = true;
                }
                continue;
            }

            let value: Value = serde_json::from_str(data).map_err(|e| ModelError {
                message: format!("malformed SSE JSON: {e}"),
                code: Some("invalid_json".into()),
                provider: None,
                status: None,
                upstream: Some(Arc::new(UpstreamErrorDetail {
                    code: None,
                    message: None,
                    body: Some(Value::String(data.to_string())),
                })),
            })?;

            if let Some(usage_value) = value.get("usage").filter(|u| !u.is_null()) {
                usage.input_tokens = usage_value
                    .get("prompt_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(usage.input_tokens);
                usage.output_tokens = usage_value
                    .get("completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(usage.output_tokens);

                if let Some(details) = usage_value.get("prompt_tokens_details") {
                    if let Some(cached) = details.get("cached_tokens").and_then(Value::as_u64) {
                        usage.cache_read_tokens = cached;
                    }
                }
                if let Some(cached) = usage_value
                    .get("prompt_cache_hit_tokens")
                    .and_then(Value::as_u64)
                {
                    usage.cache_read_tokens = cached;
                }
                if let Some(missed) = usage_value
                    .get("prompt_cache_miss_tokens")
                    .and_then(Value::as_u64)
                {
                    usage
                        .details
                        .insert("prompt_cache_miss_tokens".into(), missed);
                }
                if let Some(details) = usage_value.get("completion_tokens_details") {
                    if let Some(rt) = details.get("reasoning_tokens").and_then(Value::as_u64) {
                        usage.reasoning_tokens = rt;
                    }
                }
            }

            let Some(choice) = value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|v| v.first())
            else {
                continue;
            };

            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                stop_reason = map_stop_reason(reason);

                if reason == "tool_calls" || reason == "function_call" {
                    for tc in &tool_calls {
                        if let Some(tx) = tx {
                            let _ = tx.send(StreamEvent::ToolUseEnd { id: tc.id.clone() }).await;
                        }
                    }
                }
            }

            let Some(delta) = choice.get("delta") else {
                continue;
            };

            // Reasoning/thinking extraction
            if let Some(field_name) = reasoning_field {
                let reasoning_content =
                    delta.get(field_name).and_then(Value::as_str).or_else(|| {
                        if field_name != "reasoning_content" {
                            delta.get("reasoning_content").and_then(Value::as_str)
                        } else {
                            None
                        }
                    });

                if let Some(r_text) = reasoning_content {
                    if !thinking_active {
                        thinking_active = true;
                        if let Some(tx) = tx {
                            let _ = tx.send(StreamEvent::ThinkingStart).await;
                        }
                    }
                    thinking_text.push_str(r_text);
                    if let Some(tx) = tx {
                        let _ = tx
                            .send(StreamEvent::Thinking {
                                delta: r_text.to_string(),
                            })
                            .await;
                    }
                } else if thinking_active {
                    // Reasoning field went away — end thinking
                    thinking_active = false;

                    if let Some(details_field) = reasoning_details_field {
                        if let Some(details) = delta.get(details_field) {
                            thinking_details = Some(details.clone());
                        }
                    }
                    if let Some(tx) = tx {
                        let _ = tx
                            .send(StreamEvent::ThinkingEnd {
                                signature: None,
                                provider_details: thinking_details.clone(),
                            })
                            .await;
                    }
                }

                // Also check for reasoning_details on any delta
                if let Some(details_field) = reasoning_details_field {
                    if let Some(details) = delta.get(details_field) {
                        thinking_details = Some(details.clone());
                    }
                }
            }

            // Text content
            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                if thinking_active {
                    // End thinking before text starts
                    thinking_active = false;
                    if let Some(tx) = tx {
                        let _ = tx
                            .send(StreamEvent::ThinkingEnd {
                                signature: None,
                                provider_details: thinking_details.clone(),
                            })
                            .await;
                    }
                }
                first_token_latency.get_or_insert_with(|| start.elapsed());
                text.push_str(content);
                if let Some(tx) = tx {
                    let _ = tx
                        .send(StreamEvent::Text {
                            delta: content.to_string(),
                        })
                        .await;
                }
            }

            // Tool calls
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let idx = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    while tool_calls.len() <= idx {
                        tool_calls.push(ToolCallAccum {
                            id: String::new(),
                            name: String::new(),
                            args: String::new(),
                            started: false,
                        });
                    }
                    if let Some(id) = call.get("id").and_then(Value::as_str) {
                        tool_calls[idx].id = id.to_string();
                    }
                    if let Some(function) = call.get("function") {
                        if let Some(name) = function.get("name").and_then(Value::as_str) {
                            tool_calls[idx].name = name.to_string();
                        }
                        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                            // Emit ToolUseStart on first chunk
                            if !tool_calls[idx].started {
                                tool_calls[idx].started = true;
                                first_token_latency.get_or_insert_with(|| start.elapsed());
                                if let Some(tx) = tx {
                                    let _ = tx
                                        .send(StreamEvent::ToolUseStart {
                                            id: tool_calls[idx].id.clone(),
                                            name: tool_calls[idx].name.clone(),
                                        })
                                        .await;
                                }
                            }
                            tool_calls[idx].args.push_str(arguments);
                            if let Some(tx) = tx {
                                let _ = tx
                                    .send(StreamEvent::ToolUseArgsChunk {
                                        id: tool_calls[idx].id.clone(),
                                        delta: arguments.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    }

    // End any still-active thinking
    if thinking_active {
        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::ThinkingEnd {
                    signature: None,
                    provider_details: thinking_details.clone(),
                })
                .await;
        }
    }

    if !got_done {
        return Err(ModelError {
            message: "SSE stream ended without [DONE] signal".into(),
            code: Some("stream_interrupted".into()),
            provider: None,
            status: None,
            upstream: None,
        });
    }

    // Build content blocks
    let mut content = Vec::new();

    if !thinking_text.is_empty() {
        content.push(ContentBlock::Thinking {
            text: Some(thinking_text),
            signature: None,
            provider_details: thinking_details,
        });
    }

    if !text.is_empty() {
        content.push(ContentBlock::Text(text));
    }

    for tc in tool_calls {
        if tc.name.is_empty() {
            continue;
        }
        let input: Value = serde_json::from_str(&tc.args).map_err(|_| ModelError {
            message: format!("invalid tool call arguments for '{}': {}", tc.name, tc.args),
            code: Some("invalid_tool_arguments".into()),
            provider: None,
            status: None,
            upstream: None,
        })?;
        content.push(ContentBlock::ToolUse {
            id: tc.id,
            name: tc.name,
            input,
        });
    }

    Ok(SseParseResult {
        content,
        usage,
        stop_reason,
        first_token_latency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures_util::stream;

    fn make_stream(chunks: Vec<&str>) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Unpin {
        stream::iter(
            chunks
                .into_iter()
                .map(|s| Ok(Bytes::from(s.to_string())))
                .collect::<Vec<_>>(),
        )
    }

    fn make_stream_owned(
        chunks: Vec<String>,
    ) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Unpin {
        stream::iter(
            chunks
                .into_iter()
                .map(|s| Ok(Bytes::from(s)))
                .collect::<Vec<_>>(),
        )
    }

    #[tokio::test]
    async fn parse_buffered_chunks() {
        // Split an SSE event across two chunks
        let s = make_stream(vec![
            "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n",
            "\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n",
        ]);
        let (tx, mut rx) = mpsc::channel(16);
        let r = parse_openai_sse_stream(s, Some(&tx), None, None, Instant::now())
            .await
            .unwrap();
        let content = r.content;
        drop(tx);

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        assert!(matches!(&events[0], StreamEvent::Text { delta } if delta == "hel"));
        assert!(matches!(&events[1], StreamEvent::Text { delta } if delta == "lo"));
        assert!(matches!(&content[0], ContentBlock::Text(t) if t == "hello"));
    }

    #[tokio::test]
    async fn parse_with_reasoning_field() {
        let sse = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\n\
                   data: {\"choices\":[{\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3}}\n\n\
                   data: [DONE]\n\n";
        let s = make_stream(vec![sse]);
        let (tx, mut rx) = mpsc::channel(16);
        let SseParseResult {
            content,
            usage,
            stop_reason: stop,
            ..
        } = parse_openai_sse_stream(
            s,
            Some(&tx),
            Some("reasoning_content"),
            None,
            Instant::now(),
        )
        .await
        .unwrap();
        drop(tx);

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        assert!(matches!(events[0], StreamEvent::ThinkingStart));
        assert!(matches!(&events[1], StreamEvent::Thinking { delta } if delta == "think"));
        assert!(matches!(events[2], StreamEvent::ThinkingEnd { .. }));
        assert!(matches!(&events[3], StreamEvent::Text { delta } if delta == "answer"));

        assert_eq!(content.len(), 2);
        assert!(
            matches!(&content[0], ContentBlock::Thinking { text, .. } if text.as_deref() == Some("think"))
        );
        assert!(matches!(&content[1], ContentBlock::Text(t) if t == "answer"));
        assert_eq!(usage.input_tokens, 5);
        assert_eq!(stop, StopReason::EndTurn);
    }

    #[tokio::test]
    async fn parse_without_reasoning_field() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
                   data: {\"choices\":[{\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n\
                   data: [DONE]\n\n";
        let s = make_stream(vec![sse]);
        let (tx, mut rx) = mpsc::channel(16);
        let r = parse_openai_sse_stream(s, Some(&tx), None, None, Instant::now())
            .await
            .unwrap();
        let content = r.content;
        drop(tx);

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        assert!(!events
            .iter()
            .any(|e| matches!(e, StreamEvent::ThinkingStart)));
        assert_eq!(content.len(), 1);
    }

    #[tokio::test]
    async fn parse_tool_calls() {
        let chunk1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"echo","arguments":"{\"text\":"}}]}}]}"#;
        let chunk2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hi\"}"}}]},"finish_reason":"tool_calls"}]}"#;
        let sse = format!("{chunk1}\n\n{chunk2}\n\ndata: [DONE]\n\n");
        let s = make_stream_owned(vec![sse]);
        let (tx, mut rx) = mpsc::channel(16);
        let SseParseResult {
            content,
            stop_reason: stop,
            ..
        } = parse_openai_sse_stream(s, Some(&tx), None, None, Instant::now())
            .await
            .unwrap();
        drop(tx);

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        assert!(
            matches!(&events[0], StreamEvent::ToolUseStart { id, name } if id == "c1" && name == "echo")
        );
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseArgsChunk { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { id } if id == "c1")));
        assert_eq!(stop, StopReason::ToolUse);
        assert!(matches!(&content[0], ContentBlock::ToolUse { name, .. } if name == "echo"));
    }

    #[tokio::test]
    async fn parse_done_signal() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\ndata: [DONE]\n\n";
        let s = make_stream(vec![sse]);
        let result = parse_openai_sse_stream(s, None, None, None, Instant::now()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn parse_malformed_json() {
        let sse = "data: {NOT VALID}\n\ndata: [DONE]\n\n";
        let s = make_stream(vec![sse]);
        let err = parse_openai_sse_stream(s, None, None, None, Instant::now())
            .await
            .unwrap_err();
        assert_eq!(err.code.as_deref(), Some("invalid_json"));
    }

    #[tokio::test]
    async fn parse_stream_interrupted() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
        let s = make_stream(vec![sse]);
        let err = parse_openai_sse_stream(s, None, None, None, Instant::now())
            .await
            .unwrap_err();
        assert_eq!(err.code.as_deref(), Some("stream_interrupted"));
    }

    #[tokio::test]
    async fn parse_missing_usage() {
        let sse =
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n\
                   data: [DONE]\n\n";
        let s = make_stream(vec![sse]);
        let r = parse_openai_sse_stream(s, None, None, None, Instant::now())
            .await
            .unwrap();
        let usage = r.usage;
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }
}
