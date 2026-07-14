//! The shared Chat Completions protocol core (ADR-0002 Phase 3).
//!
//! One [`ChatAdapter`] serves every Chat provider (OpenAI, DeepSeek, Volcengine,
//! OpenRouter). The canonical envelope (message mapping, tool-call assembly, SSE
//! decode, `stream_options.include_usage`) lives here; every per-provider
//! divergence is carried as data/behavior on the [`ProviderProfile`] attached to
//! the resolved entry — there are **no provider-name conditionals** in this core.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use orchest_protocol::ProtocolError;
use orchest_provider_core::registry::ProviderConfig;

use crate::catalog::LlmModelEntry;
use crate::protocol::{
    normalize_chat_stop_reason, resolve_chat_preflight, resolve_headers, Protocol, ProviderEntry,
    ProviderProfile, ResolvedModel, CANONICAL_CHAT,
};
use crate::role_compat::{downgrade_minimax_role, CompatibleRole};
use crate::{
    telemetry, ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    OptionAdjustment, RequestOptions, StreamEvent, ToolDef, UpstreamErrorDetail,
};

/// The shared Chat adapter. Constructed by each provider's `build_chat_adapter`
/// with a fully resolved endpoint/headers and the provider's profile.
pub struct ChatAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
    entry: &'static ProviderEntry,
    catalog: Option<&'static LlmModelEntry>,
    profile: &'static dyn ProviderProfile,
    extra_headers: Vec<(&'static str, String)>,
}

impl std::fmt::Debug for ChatAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatAdapter")
            .field("provider", &self.entry.name)
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl ChatAdapter {
    /// Build the Chat adapter for `resolved` provider/model. `api_url` is the
    /// already-resolved complete endpoint (the caller applies the provider's
    /// base-URL + path rules); `extra_headers` are resolved from the entry.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
    pub fn build(
        config: &ProviderConfig,
        resolved: &ResolvedModel<'_>,
        api_key: String,
        api_url: String,
    ) -> Result<Box<dyn ModelAdapter>, ProtocolError> {
        let profile = resolved
            .provider
            .profile_for(Protocol::Chat)
            .unwrap_or(&CANONICAL_CHAT);
        Ok(Box::new(ChatAdapter {
            api_key,
            api_url,
            model: resolved.model.to_string(),
            max_tokens: config.max_tokens.unwrap_or(crate::defaults::MAX_TOKENS),
            entry: resolved.provider,
            catalog: resolved.catalog,
            profile,
            extra_headers: resolve_headers(resolved.provider),
        }))
    }

    fn cx(&self) -> ResolvedModel<'_> {
        ResolvedModel {
            provider: self.entry,
            protocol: Protocol::Chat,
            model: &self.model,
            catalog: self.catalog,
        }
    }

    /// Build a Chat adapter for `provider`/`model` with an explicit endpoint, for
    /// the per-provider request/response assertion suites.
    #[cfg(test)]
    pub(crate) fn for_test(provider: &str, model: &str, api_url: &str, max_tokens: u32) -> Self {
        let entry = crate::protocol::provider_entry(provider).expect("provider entry");
        let profile = entry.profile_for(Protocol::Chat).unwrap_or(&CANONICAL_CHAT);
        ChatAdapter {
            api_key: "test-key".to_string(),
            api_url: api_url.to_string(),
            model: model.to_string(),
            max_tokens,
            entry,
            catalog: crate::catalog::find_model(model),
            profile,
            extra_headers: resolve_headers(entry),
        }
    }

    /// Build the request body from **effective** options (post-preflight). The
    /// message envelope is canonical; the profile injects reasoning replay and
    /// lowers the remaining options (reasoning dialect / sampling / cache).
    #[allow(clippy::result_large_err)] // justified: ModelError carries diagnostic context (workspace convention)
    fn build_request_body(
        &self,
        effective_options: &RequestOptions,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<(Value, Vec<OptionAdjustment>), ModelError> {
        let cx = self.cx();
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
                            } => tool_results.push((tool_use_id.clone(), content.clone())),
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
                        api_messages
                            .push(json!({"role": "user", "content": text_parts.join("\n")}));
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
                                    "function": {"name": name, "arguments": input.to_string()}
                                }));
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
                    // Reasoning replay is the profile's job (canonical drops it).
                    self.profile
                        .replay_reasoning(&cx, &mut msg, &message.content)?;
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

        let effective_max_tokens = effective_options.max_tokens.unwrap_or(self.max_tokens);
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

        adjustments.extend(
            self.profile
                .lower_options(&cx, effective_options, &mut body),
        );
        Ok((body, adjustments))
    }

    /// Pre-flight + build, combined — the request body plus every request-side
    /// adjustment. Used by the per-provider request assertion suites.
    #[cfg(test)]
    #[allow(clippy::result_large_err)] // justified: ModelError carries diagnostic context (workspace convention)
    pub(crate) fn request_body_for_test(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
    ) -> Result<(Value, Vec<OptionAdjustment>), ModelError> {
        let cx = self.cx();
        let (effective, mut adjustments) = resolve_chat_preflight(self.profile, &cx, options)?;
        let (body, body_adj) = self.build_request_body(&effective, messages, tools)?;
        adjustments.extend(body_adj);
        Ok((body, adjustments))
    }
}

#[async_trait]
impl ModelAdapter for ChatAdapter {
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

        // Uniform CompatibilityPolicy handling (Strict errors / degrades).
        let (effective_options, mut option_adjustments) =
            resolve_chat_preflight(self.profile, &cx, options)?;

        let (body, body_adjustments) =
            self.build_request_body(&effective_options, messages, tools)?;
        option_adjustments.extend(body_adjustments);

        let mut request = crate::http::shared_client()
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body);
        for (name, value) in &self.extra_headers {
            request = request.header(*name, value);
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
                        err.get("type")
                            .or_else(|| err.get("code"))
                            .and_then(|v| v.as_str())
                            .map(String::from),
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

        let stream = response.bytes_stream();
        let tx_ref = tx.as_ref();
        let (reasoning_field, reasoning_details_field) = self.profile.chat_sse_reasoning(&cx);
        let sse = crate::sse::parse_openai_sse_stream(
            stream,
            tx_ref,
            reasoning_field,
            reasoning_details_field,
            start,
        )
        .await
        .map_err(|mut e| {
            telemetry::record_model_error(provider, &self.model, start.elapsed());
            e.provider = Some(provider.into());
            e
        })?;

        let content = sse.content;
        let mut usage = sse.usage;
        let stop_reason = normalize_chat_stop_reason(sse.stop_reason);
        let first_token_latency = sse.first_token_latency;

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
            first_token_latency,
            Some(duration),
        );

        Ok(ModelResponse {
            content,
            usage,
            stop_reason,
            option_adjustments,
        })
    }
}
