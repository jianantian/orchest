//! Builds the Anthropic Messages API request body: message/content mapping,
//! thinking (adaptive vs. budget-token) mode, cache control, and sampling
//! params. Pure data transformation — no networking, kept separate from the
//! adapter shell and the SSE response consumer.

use serde_json::{json, Value};

use crate::{
    CachePolicy, ContentBlock, Message, OptionAdjustment, RequestOptions, Role, ThinkingLevel,
    ToolDef,
};

use super::AnthropicAdapter;

pub(super) fn normalize_messages_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1/messages") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1/messages")
    }
}

impl AnthropicAdapter {
    pub(super) fn build_request_body(
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
