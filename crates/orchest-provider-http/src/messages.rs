//! The shared Anthropic-Messages protocol core (ADR-0002 Phase 3).
//!
//! One `MessagesAdapter` serves every Messages provider (Anthropic, Minimax).
//! The canonical envelope (system field, message/content mapping, the
//! `thinking: {type, display}` + `output_config` dialect, cache control, sampling,
//! SSE decode) lives here; per-provider divergence rides on the
//! `ProviderProfile` attached to the entry — role mapping (`messages_wire_role`),
//! multimodal content encoding (`encode_multimodal_block`), adaptive-thinking
//! detection (`messages_supports_adaptive`), auth headers (`messages_auth_headers`),
//! and `capabilities`. There are **no provider-name conditionals** in this core.

pub(crate) mod response;

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use orchest_protocol::ProtocolError;
use orchest_provider_core::registry::ProviderConfig;

use crate::catalog::LlmModelEntry;
use crate::protocol::{
    Protocol, ProviderEntry, ProviderProfile, ResolvedModel, CANONICAL_MESSAGES,
};
use crate::{
    telemetry, CachePolicy, ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError,
    ModelResponse, OptionAdjustment, RequestOptions, Role, StreamEvent, ThinkingLevel, ToolDef,
    UpstreamErrorDetail,
};

use response::consume_event_stream;

/// The shared Messages adapter.
pub struct MessagesAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    entry: &'static ProviderEntry,
    catalog: Option<&'static LlmModelEntry>,
    profile: &'static dyn ProviderProfile,
}

impl std::fmt::Debug for MessagesAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessagesAdapter")
            .field("provider", &self.entry.name)
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl MessagesAdapter {
    /// Build the Messages adapter for `resolved` provider/model. `api_url` is the
    /// already-resolved complete endpoint.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
    pub fn build(
        config: &ProviderConfig,
        resolved: &ResolvedModel<'_>,
        api_key: String,
        api_url: String,
    ) -> Result<Box<dyn ModelAdapter>, ProtocolError> {
        let profile = resolved
            .provider
            .profile_for(Protocol::Messages)
            .unwrap_or(&CANONICAL_MESSAGES);
        Ok(Box::new(MessagesAdapter {
            api_key,
            api_url,
            model: resolved.model.to_string(),
            max_tokens: config.max_tokens.unwrap_or(crate::defaults::MAX_TOKENS),
            entry: resolved.provider,
            catalog: resolved.catalog,
            profile,
        }))
    }

    fn cx(&self) -> ResolvedModel<'_> {
        ResolvedModel {
            provider: self.entry,
            protocol: Protocol::Messages,
            model: &self.model,
            catalog: self.catalog,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(provider: &str, model: &str, api_url: &str, max_tokens: u32) -> Self {
        let entry = crate::protocol::provider_entry(provider).expect("provider entry");
        let profile = entry
            .profile_for(Protocol::Messages)
            .unwrap_or(&CANONICAL_MESSAGES);
        MessagesAdapter {
            api_key: "test-key".to_string(),
            api_url: api_url.to_string(),
            model: model.to_string(),
            max_tokens,
            entry,
            catalog: crate::catalog::find_model(model),
            profile,
        }
    }

    fn content_blocks(
        &self,
        cx: &ResolvedModel<'_>,
        blocks: &[ContentBlock],
        adjustments: &mut Vec<OptionAdjustment>,
    ) -> Vec<Value> {
        let mut content: Vec<Value> = Vec::with_capacity(blocks.len());
        for block in blocks {
            match block {
                ContentBlock::Text(t) => content.push(json!({"type": "text", "text": t})),
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
                ContentBlock::ToolUse { id, name, input } => content.push(json!({
                    "type": "tool_use", "id": id, "name": name, "input": input
                })),
                ContentBlock::ToolResult {
                    tool_use_id,
                    content: tr,
                } => content.push(json!({
                    "type": "tool_result", "tool_use_id": tool_use_id, "content": tr
                })),
                // Multimodal blocks are the profile's content-encoding deviation.
                ContentBlock::Image { .. }
                | ContentBlock::Video { .. }
                | ContentBlock::Audio { .. }
                | ContentBlock::MidConvSystem(_) => {
                    if let Some(v) = self.profile.encode_multimodal_block(cx, block, adjustments) {
                        content.push(v);
                    }
                }
            }
        }
        content
    }

    fn build_request_body(
        &self,
        options: &RequestOptions,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> (Value, Vec<OptionAdjustment>) {
        let cx = self.cx();
        let mut system_parts = Vec::new();
        let mut api_messages = Vec::new();
        let mut adjustments = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                for block in &msg.content {
                    if let ContentBlock::Text(t) = block {
                        system_parts.push(t.clone());
                    }
                }
                continue;
            }
            let api_role = self
                .profile
                .messages_wire_role(&cx, &msg.role, &mut adjustments);
            let content = self.content_blocks(&cx, &msg.content, &mut adjustments);
            api_messages.push(json!({"role": api_role, "content": content}));
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
            body["tools"] = json!(tools
                .iter()
                .map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                }))
                .collect::<Vec<_>>());
        }

        // Thinking dialect (adaptive vs budget_tokens) — canonical Messages,
        // gated only by the profile's adaptive detection.
        match options.thinking {
            ThinkingLevel::Off => {
                body["thinking"] = json!({"type": "disabled"});
            }
            level => {
                let display = if options.include_thinking {
                    "summarized"
                } else {
                    "omitted"
                };
                if self.profile.messages_supports_adaptive(&cx) {
                    let effort = match level {
                        ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                        ThinkingLevel::Medium => "medium",
                        ThinkingLevel::High => "high",
                        ThinkingLevel::XHigh => "xhigh",
                        ThinkingLevel::Max => "max",
                        ThinkingLevel::Off => unreachable!(),
                    };
                    body["thinking"] = json!({"type": "adaptive"});
                    body["thinking"]["display"] = json!(display);
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
                    body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
                    body["thinking"]["display"] = json!(display);
                }
            }
        }

        match options.cache_policy {
            CachePolicy::Auto => body["cache_control"] = json!({"type": "ephemeral"}),
            CachePolicy::Long => body["cache_control"] = json!({"type": "ephemeral", "ttl": "1h"}),
            CachePolicy::None => {}
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(tp) = options.top_p {
            body["top_p"] = json!(tp);
        }

        // service_tier pass-through: a Messages-wire meta-option. Only present when
        // the caller sets it (Minimax gateway tiers); Anthropic callers leave it
        // None, so this stays byte-identical for Anthropic.
        if let Some(tier) = &options.service_tier {
            body["service_tier"] = json!(tier);
        }

        (body, adjustments)
    }

    #[cfg(test)]
    pub(crate) fn request_body_for_test(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> (Value, Vec<OptionAdjustment>) {
        self.build_request_body(options, messages, tools)
    }
}

#[async_trait]
impl ModelAdapter for MessagesAdapter {
    fn provider_name(&self) -> &str {
        self.entry.name
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        self.profile.capabilities(&self.cx(), self.max_tokens)
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let provider = self.entry.name;
        let _span = telemetry::model_complete_span(provider, &self.model, tx.is_some());
        let cx = self.cx();

        let (body, mut option_adjustments) = self.build_request_body(options, messages, tools);

        // thinking_budget under adaptive thinking is a hard error under Strict.
        if options.compatibility_policy == crate::CompatibilityPolicy::Strict
            && options.thinking_budget_tokens.is_some()
            && self.profile.messages_supports_adaptive(&cx)
            && options.thinking != ThinkingLevel::Off
        {
            return Err(ModelError::internal(
                "thinking_budget_tokens is not supported in adaptive thinking mode",
                "unsupported_thinking_budget",
            ));
        }

        let mut request = crate::http::shared_client().post(&self.api_url).json(&body);
        for (name, value) in self.profile.messages_auth_headers(&cx, &self.api_key) {
            request = request.header(name, value);
        }

        let start = Instant::now();
        let response = request.send().await.map_err(|e| {
            telemetry::record_model_error(provider, &self.model, start.elapsed());
            ModelError {
                message: e.to_string(),
                code: Some("request_failed".into()),
                provider: Some(provider.into()),
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
            telemetry::record_model_error(provider, &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some(provider.into()),
                status: Some(status),
                retry_after_secs: None,
                upstream: Some(Arc::new(UpstreamErrorDetail {
                    code: upstream_code,
                    message: upstream_msg,
                    body: upstream_body,
                })),
            });
        }

        let outcome = consume_event_stream(response.bytes_stream(), provider, tx.as_ref(), start)
            .await
            .map_err(|mut e| {
                telemetry::record_model_error(provider, &self.model, start.elapsed());
                e.provider.get_or_insert_with(|| provider.into());
                e
            })?;

        let mut usage = outcome.usage;
        option_adjustments.extend(self.profile.interpret_usage(&cx, &Value::Null, &mut usage));

        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let duration = start.elapsed();
        telemetry::record_model_success(
            provider,
            &self.model,
            duration,
            usage.input_tokens,
            usage.output_tokens,
            outcome.first_token_latency,
            Some(duration),
        );

        usage.cost_usd = self
            .profile
            .capabilities(&cx, self.max_tokens)
            .pricing
            .map(|p| p.calculate(&usage));

        Ok(ModelResponse {
            content: outcome.content,
            usage,
            stop_reason: outcome.stop_reason,
            option_adjustments,
        })
    }
}
