//! Builds the OpenAI chat completions request body: message/content mapping,
//! reasoning effort, cache policy, and sampling params. Pure data
//! transformation — no networking, kept separate from the adapter shell.

use serde_json::{json, Value};

use crate::{
    CachePolicy, ContentBlock, Message, OptionAdjustment, RequestOptions, ThinkingLevel, ToolDef,
};

use crate::role_compat::{downgrade_minimax_role, CompatibleRole};

use super::OpenAiAdapter;

pub(super) fn normalize_chat_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

pub(super) fn supports_reasoning_model(model: &str) -> bool {
    let name = model.split_once('/').map_or(model, |(_, m)| m);
    name.starts_with("o1")
        || name.starts_with("o3")
        || name.starts_with("o4")
        || name.starts_with("gpt-5")
}

pub(super) fn openai_context_window(model: &str) -> u64 {
    let name = model.split_once('/').map_or(model, |(_, m)| m);
    if name.starts_with("gpt-5.5") || name == "gpt-5.4" {
        1_000_000
    } else if name.starts_with("gpt-5.4-mini") || name.starts_with("gpt-5.4-nano") {
        400_000
    } else {
        128_000
    }
}

impl OpenAiAdapter {
    pub(super) fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> (Value, Vec<OptionAdjustment>) {
        let mut api_messages: Vec<Value> = Vec::new();
        let mut adjustments = Vec::new();

        for message in messages {
            let effective_role = downgrade_minimax_role(message.role, &mut adjustments);
            match effective_role {
                CompatibleRole::System => {
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
                CompatibleRole::User => {
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
                CompatibleRole::Assistant => {
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
                CompatibleRole::Tool => {
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
