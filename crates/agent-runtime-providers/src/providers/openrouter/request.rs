//! Builds the OpenRouter chat/completions request body: message/content
//! mapping, reasoning replay, thinking-level mapping, and sampling params.
//! Pure data transformation — no networking, kept separate from the adapter
//! shell.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::{
    ContentBlock, Message, ModelError, OptionAdjustment, RequestOptions, Role, ThinkingLevel,
    ToolDef, UpstreamErrorDetail,
};

use super::OpenRouterAdapter;

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

pub(super) fn append_reasoning_details(
    target: &mut Vec<Value>,
    details: &Value,
) -> Result<(), ModelError> {
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

impl OpenRouterAdapter {
    #[cfg(test)]
    pub(super) fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> (Value, Vec<OptionAdjustment>) {
        self.try_build_request_body(messages, tools, options)
            .expect("valid OpenRouter reasoning replay")
    }

    pub(super) fn try_build_request_body(
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
