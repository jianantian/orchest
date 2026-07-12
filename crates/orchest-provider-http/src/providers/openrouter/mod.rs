//! OpenRouter adapter implementation (multi-provider routing).

mod profile;
mod request;

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CapabilitySource, Message, ModelAdapter, ModelCapabilities, ModelError,
    ModelResponse, ReasoningCapability, RequestOptions, StreamEvent, ThinkingLevel, ToolDef,
    UpstreamErrorDetail,
};

use crate::catalog::LlmModelEntry;
use crate::protocol::{Protocol, ProviderEntry, ProviderProfile, ResolvedModel};
use crate::{defaults, telemetry};

pub use profile::{OpenRouterProfile, OPENROUTER_PROFILE};
use request::normalize_chat_url;

pub struct OpenRouterAdapter {
    pub(super) api_key: String,
    pub(super) api_url: String,
    pub(super) model: String,
    pub(super) max_tokens: u32,
    /// Routing headers, resolved from the entry (or config) at construction.
    pub(super) extra_headers: Vec<(&'static str, String)>,
    /// ADR-0002 resolution context for the profile hooks (see `cx`).
    entry: &'static ProviderEntry,
    catalog: Option<&'static LlmModelEntry>,
    profile: &'static dyn ProviderProfile,
}

impl std::fmt::Debug for OpenRouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct OpenRouterConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    /// Pre-resolved routing headers (`(name, value)`). On the protocol-factory
    /// path these come from `protocol::resolve_headers` applied to the entry's
    /// `HeaderValue::Env` declarations.
    pub extra_headers: Vec<(&'static str, String)>,
}

impl OpenRouterAdapter {
    pub fn from_config(config: OpenRouterConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::openrouter::API_KEY_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "OPENROUTER_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .unwrap_or_else(|| defaults::openrouter::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "OpenRouter API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        let entry = crate::protocol::provider_entry("openrouter")
            .expect("openrouter entry is registered on the protocol path");
        let catalog = crate::catalog::find_model(&config.model);
        let profile = entry
            .profile_for(Protocol::Chat)
            .expect("openrouter entry carries a Chat profile");

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
            extra_headers: config.extra_headers,
            entry,
            catalog,
            profile,
        })
    }

    /// The ADR-0002 resolution context for this adapter, rebuilt per request.
    pub(super) fn cx(&self) -> ResolvedModel<'_> {
        ResolvedModel {
            provider: self.entry,
            protocol: Protocol::Chat,
            model: &self.model,
            catalog: self.catalog,
        }
    }
}

#[async_trait]
impl ModelAdapter for OpenRouterAdapter {
    fn provider_name(&self) -> &str {
        "openrouter"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts: vec![
                    ThinkingLevel::Minimal,
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                    ThinkingLevel::Max,
                ],
                budget_tokens: true,
                output_exclusion: true,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: None,
            source: CapabilitySource::Assumed,
            pricing: None,
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("openrouter", &self.model, tx.is_some());

        let (body, mut option_adjustments) =
            self.try_build_request_body(messages, tools, options)?;

        let mut request = crate::http::shared_client()
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body);

        for (name, value) in &self.extra_headers {
            request = request.header(*name, value);
        }

        let start = Instant::now();
        let response = request.send().await.map_err(|e| {
            telemetry::record_model_error("openrouter", &self.model, start.elapsed());
            ModelError {
                message: e.to_string(),
                code: Some("request_failed".into()),
                provider: Some("openrouter".into()),
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

            telemetry::record_model_error("openrouter", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("openrouter".into()),
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

        let sse = crate::sse::parse_openai_sse_stream(
            stream,
            tx_ref,
            Some("reasoning"),
            Some("reasoning_details"),
            start,
        )
        .await
        .map_err(|mut e| {
            telemetry::record_model_error("openrouter", &self.model, start.elapsed());
            e.provider = Some("openrouter".into());
            e
        })?;

        let content = sse.content;
        let mut usage = sse.usage;
        let stop_reason = sse.stop_reason;
        let first_token_latency = sse.first_token_latency;

        // Usage-missing handling is the canonical interpret_usage default,
        // shared with OpenAI/DeepSeek.
        option_adjustments.extend(self.profile.interpret_usage(
            &self.cx(),
            &Value::Null,
            &mut usage,
        ));

        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let duration = start.elapsed();
        telemetry::record_model_success(
            "openrouter",
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

// ADR-0002 protocol-factory path (slice 004). Referenced as data by the
// OpenRouter `ProviderEntry.build_chat`; the factory never matches on provider
// name (ADR rule 1). Routing headers are resolved from the entry's
// HeaderValue::Env declarations here. Transitional wrapping; collapsed in v0.12.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_chat_adapter(
    config: &orchest_provider_core::registry::ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn orchest_protocol::ChatModel>, orchest_protocol::ProtocolError> {
    let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
        model: resolved.model.to_string(),
        max_tokens: config.max_tokens.unwrap_or(crate::defaults::MAX_TOKENS),
        api_key: config.api_key.clone(),
        api_url: config.api_url.clone(),
        extra_headers: crate::protocol::resolve_headers(resolved.provider),
    })
    .map_err(orchest_protocol::ProtocolError::from)?;
    Ok(Box::new(adapter))
}

#[cfg(test)]
mod tests;
