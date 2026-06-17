//! Consumes the Anthropic Messages API's SSE stream into a normalized
//! [`StreamOutcome`], independent of the adapter struct (it only needs a
//! byte stream, a provider tag for error attribution, and an optional event
//! sender) — kept separate from request building and the adapter shell.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{ContentBlock, ModelError, StopReason, StreamEvent, TokenUsage, UpstreamErrorDetail};

pub(super) fn map_stop_reason(raw: &str) -> StopReason {
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

pub(super) struct StreamOutcome {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    pub first_token_latency: Option<Duration>,
}

// Accumulates per-content-block state across the three SSE events that
// bracket each block (content_block_start / content_block_delta* / content_block_stop).
struct BlockState {
    block_type: Option<String>,
    block_id: Option<String>,
    block_name: Option<String>,
    text: String,
    thinking_text: String,
    tool_input_json: String,
}

impl BlockState {
    fn new() -> Self {
        Self {
            block_type: None,
            block_id: None,
            block_name: None,
            text: String::new(),
            thinking_text: String::new(),
            tool_input_json: String::new(),
        }
    }

    async fn on_start(&mut self, block: &Value, tx: Option<&mpsc::Sender<StreamEvent>>) {
        self.block_type = block.get("type").and_then(|v| v.as_str()).map(String::from);
        self.block_id = block.get("id").and_then(|v| v.as_str()).map(String::from);
        self.block_name = block.get("name").and_then(|v| v.as_str()).map(String::from);
        self.text.clear();
        self.thinking_text.clear();
        self.tool_input_json.clear();

        if let Some(tx) = tx {
            match self.block_type.as_deref() {
                Some("thinking") => {
                    let _ = tx.send(StreamEvent::ThinkingStart).await;
                }
                Some("tool_use") => {
                    let _ = tx
                        .send(StreamEvent::ToolUseStart {
                            id: self.block_id.clone().unwrap_or_default(),
                            name: self.block_name.clone().unwrap_or_default(),
                        })
                        .await;
                }
                _ => {}
            }
        }
    }

    async fn on_delta(
        &mut self,
        delta: &Value,
        first_token_latency: &mut Option<Duration>,
        start: Instant,
        tx: Option<&mpsc::Sender<StreamEvent>>,
    ) {
        match delta.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "text_delta" => {
                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                    first_token_latency.get_or_insert_with(|| start.elapsed());
                    self.text.push_str(text);
                    if let Some(tx) = tx {
                        let _ = tx.send(StreamEvent::Text { delta: text.to_string() }).await;
                    }
                }
            }
            "input_json_delta" => {
                if let Some(partial) = delta.get("partial_json").and_then(|v| v.as_str()) {
                    first_token_latency.get_or_insert_with(|| start.elapsed());
                    self.tool_input_json.push_str(partial);
                    if let Some(tx) = tx {
                        if let Some(id) = &self.block_id {
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
                if let Some(text) = delta.get("thinking").and_then(|v| v.as_str()) {
                    self.thinking_text.push_str(text);
                    if let Some(tx) = tx {
                        let _ = tx.send(StreamEvent::Thinking { delta: text.to_string() }).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn on_stop(
        &mut self,
        data: &Value,
        content_blocks: &mut Vec<ContentBlock>,
        tx: Option<&mpsc::Sender<StreamEvent>>,
    ) {
        match self.block_type.as_deref() {
            Some("text") => {
                content_blocks.push(ContentBlock::Text(self.text.clone()));
            }
            Some("tool_use") => {
                let input: Value = serde_json::from_str(&self.tool_input_json)
                    .unwrap_or(Value::Object(Default::default()));
                let id = self.block_id.clone().unwrap_or_default();
                content_blocks.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: self.block_name.clone().unwrap_or_default(),
                    input,
                });
                if let Some(tx) = tx {
                    let _ = tx.send(StreamEvent::ToolUseEnd { id }).await;
                }
            }
            Some("thinking") => {
                let signature = data
                    .get("content_block")
                    .and_then(|b| b.get("signature"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let thinking_text = (!self.thinking_text.is_empty())
                    .then(|| self.thinking_text.clone());
                content_blocks.push(ContentBlock::Thinking {
                    text: thinking_text,
                    signature: signature.clone(),
                    provider_details: None,
                });
                if let Some(tx) = tx {
                    let _ = tx
                        .send(StreamEvent::ThinkingEnd { signature, provider_details: None })
                        .await;
                }
            }
            _ => {}
        }
        self.block_type = None;
        self.block_id = None;
        self.block_name = None;
    }
}

/// Drains an Anthropic Messages API SSE byte stream, forwarding deltas to
/// `tx` as they arrive (if present) and accumulating the final content
/// blocks / usage / stop reason for the non-streaming `ModelResponse`.
pub(super) async fn consume_event_stream(
    mut stream: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
    provider: &str,
    tx: Option<&mpsc::Sender<StreamEvent>>,
    start: Instant,
) -> Result<StreamOutcome, ModelError> {
    let mut buffer = String::new();
    let mut block = BlockState::new();
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    let mut usage = TokenUsage::default();
    let mut stop_reason = StopReason::EndTurn;
    let mut got_message_stop = false;
    let mut first_token_latency: Option<Duration> = None;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| ModelError {
            message: e.to_string(),
            code: Some("stream_error".into()),
            provider: Some(provider.into()),
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
                provider: Some(provider.into()),
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
                    if let Some(b) = data.get("content_block") {
                        block.on_start(b, tx).await;
                    }
                }
                "content_block_delta" => {
                    if let Some(delta) = data.get("delta") {
                        block.on_delta(delta, &mut first_token_latency, start, tx).await;
                    }
                }
                "content_block_stop" => {
                    block.on_stop(&data, &mut content_blocks, tx).await;
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
                            if let Some(cw) =
                                u.get("cache_creation_input_tokens").and_then(|v| v.as_u64())
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
            provider: Some(provider.into()),
            status: None,
            retry_after_secs: None,
            upstream: None,
        });
    }

    Ok(StreamOutcome { content: content_blocks, usage, stop_reason, first_token_latency })
}
