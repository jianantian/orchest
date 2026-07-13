//! Builds the Minimax Messages API request body(`POST /anthropic/v1/messages`,
//! Anthropic 兼容):message/content mapping、thinking (adaptive vs. budget-token)、
//! cache control、sampling params、`service_tier` 透传、Minimax-only role + 多模态
//! block 序列化。纯数据转换,无网络调用,与 adapter shell 及 SSE 响应消费者解耦。

use serde_json::{json, Value};

use crate::{ContentBlock, MediaSource, Message, OptionAdjustment, RequestOptions, Role, ToolDef};

use super::MinimaxAdapter;

pub(super) fn normalize_messages_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/anthropic/v1/messages") || trimmed.ends_with("/v1/messages") {
        trimmed.to_string()
    } else {
        // Minimax 的 Anthropic 兼容路径是 `/anthropic/v1/messages`(`llm/api.md:42`),
        // 与 Anthropic 自家的 `/v1/messages` 不同。用户给 base URL 时自动补全。
        format!("{trimmed}/anthropic/v1/messages")
    }
}

impl MinimaxAdapter {
    pub(super) fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> (Value, Vec<OptionAdjustment>) {
        let mut system_parts = Vec::new();
        let mut api_messages = Vec::new();
        let mut adjustments = Vec::new();
        let cx = self.cx();

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
                    // Role mapping is Minimax's profile deviation (native roles).
                    let api_role = self.profile.map_role(&cx, &msg.role).0;
                    let content = build_content_blocks(&msg.content, &mut adjustments);
                    api_messages.push(json!({"role": api_role, "content": content}));
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

        // Thinking dialect (`thinking: {type, display}`), cache control, sampling,
        // and service_tier are Minimax's profile deviation. The budget-token
        // ceiling reads `body["max_tokens"]`, set just above.
        adjustments.extend(self.profile.lower_options(&cx, options, &mut body));

        (body, adjustments)
    }
}

/// 把 `Vec<ContentBlock>` 序列化成 Minimax Messages API 的 content 数组。
/// `Image` / `Video` / `MidConvSystem` 走真实序列化(锚点 `llm/api.md:1136-1321`);
/// `Audio` 在当前 LLM API 不被接受(Step 2 omni 占位),丢弃并记录 OptionAdjustment。
fn build_content_blocks(
    blocks: &[ContentBlock],
    adjustments: &mut Vec<OptionAdjustment>,
) -> Vec<Value> {
    let mut content: Vec<Value> = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            ContentBlock::Text(t) => {
                content.push(json!({"type": "text", "text": t}));
            }
            ContentBlock::Thinking {
                text, signature, ..
            } => {
                let mut obj = json!({"type": "thinking"});
                if let Some(t) = text {
                    obj["thinking"] = json!(t);
                }
                if let Some(s) = signature {
                    obj["signature"] = json!(s);
                }
                content.push(obj);
            }
            ContentBlock::ToolUse { id, name, input } => {
                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input
                }));
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content: tr_content,
            } => {
                content.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": tr_content
                }));
            }
            // Minimax 原生支持 image,与 Anthropic 同 schema(`llm/api.md:1215-1305`)。
            ContentBlock::Image { source, detail } => {
                let source_value = build_media_source_value(source);
                let mut obj = json!({"type": "image", "source": source_value});
                if let Some(d) = detail {
                    obj["detail"] = json!(d);
                }
                content.push(obj);
            }
            // Minimax 视频 block,Minimax 专属字段 fps / max_long_side_pixel
            // (`llm/api.md:1334-1343`)。
            ContentBlock::Video {
                source,
                fps,
                detail,
                max_long_side_pixel,
            } => {
                let source_value = build_media_source_value(source);
                let mut obj = json!({"type": "video", "source": source_value});
                if let Some(f) = fps {
                    obj["fps"] = json!(f);
                }
                if let Some(d) = detail {
                    obj["detail"] = json!(d);
                }
                if let Some(m) = max_long_side_pixel {
                    obj["max_long_side_pixel"] = json!(m);
                }
                content.push(obj);
            }
            // Minimax LLM API 当前不接 audio block(Step 2 omni 占位);记录后丢弃。
            ContentBlock::Audio { .. } => {
                adjustments.push(OptionAdjustment {
                    option: "content_block".into(),
                    requested: json!("audio"),
                    applied: json!(null),
                    reason: "minimax_audio_block_unsupported_in_llm_api".into(),
                });
            }
            // Minimax 对话中途插入的系统指令(`llm/api.md:1202-1211`)。
            ContentBlock::MidConvSystem(text) => {
                content.push(json!({"type": "mid_conv_system", "text": text}));
            }
        }
    }
    content
}

/// 序列化 `MediaSource` 成 Minimax `{type:url|base64}` source 对象
/// (`docs/external/minimax/llm/api.md:1245-1305`)。
fn build_media_source_value(source: &MediaSource) -> Value {
    match source {
        MediaSource::Url { url } => json!({"type": "url", "url": url}),
        MediaSource::Base64 { media_type, data } => json!({
            "type": "base64",
            "media_type": media_type,
            "data": data,
        }),
    }
}
