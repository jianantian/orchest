//! Volcengine Ark (火山方舟) adapter — OpenAI-compatible Chat Completions.
//!
//! API base: <https://ark.cn-beijing.volces.com/api/v3/chat/completions>
//! Auth:     Authorization: Bearer $ARK_API_KEY
//! Models:   doubao-seed-2-1-pro-260628, doubao-seed-2-1-turbo-260628, doubao-seed-character-260628, etc.
//!
//! Split by concern: this file owns the adapter struct, capability reporting,
//! and `complete()`'s control flow; `request` builds the Chat Completions
//! request body and helpers.

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
    ModelResponse, ReasoningCapability, RequestOptions, StopReason, StreamEvent, ThinkingLevel,
    ToolDef, UpstreamErrorDetail,
};

use crate::catalog::LlmModelEntry;
use crate::protocol::{Protocol, ProviderEntry, ProviderProfile, ResolvedModel};
use crate::{defaults, telemetry};

pub use profile::{VolcengineProfile, VOLCENGINE_PROFILE};
use request::map_stop_reason;
pub(crate) use request::normalize_chat_url;

pub struct VolcengineAdapter {
    api_key: String,
    pub(super) api_url: String,
    model: String,
    max_tokens: u32,
    /// ADR-0002 resolution context for the profile hooks (see `cx`).
    entry: &'static ProviderEntry,
    catalog: Option<&'static LlmModelEntry>,
    profile: &'static dyn ProviderProfile,
}

impl std::fmt::Debug for VolcengineAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolcengineAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct VolcengineConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl VolcengineAdapter {
    pub fn from_config(config: VolcengineConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::volcengine::API_KEY_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "ARK_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .unwrap_or_else(|| defaults::volcengine::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "Volcengine API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        let model = config
            .model
            .strip_prefix("volcengine/")
            .unwrap_or(&config.model)
            .to_string();
        let entry = crate::protocol::provider_entry("volcengine")
            .expect("volcengine entry is registered on the protocol path");
        let catalog = crate::catalog::find_model(&model);
        let profile = entry
            .profile_for(Protocol::Chat)
            .expect("volcengine entry carries a Chat profile");

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model,
            max_tokens: config.max_tokens,
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

    /// Resolve this adapter's catalog entry, if any.
    ///
    /// Returns `None` for unrecognised model ids (custom/preview endpoints
    /// keyed off the same OpenAI-compatible API). Callers MUST treat that
    /// case conservatively rather than fall back to name-prefix guessing.
    fn catalog_entry(&self) -> Option<&'static crate::catalog::LlmModelEntry> {
        self.catalog
    }

    pub(super) fn supports_thinking(&self) -> bool {
        self.catalog_entry()
            .map(|m| m.thinking.is_some())
            .unwrap_or(false)
    }
}

#[async_trait]
impl ModelAdapter for VolcengineAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let thinks = self.supports_thinking();
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: thinks,
                efforts: if thinks {
                    vec![ThinkingLevel::High, ThinkingLevel::Max]
                } else {
                    vec![]
                },
                budget_tokens: false,
                output_exclusion: false,
                replay_metadata_required: false,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: self
                .catalog_entry()
                .map(|m| m.context_window)
                .or(Some(128_000)),
            source: CapabilitySource::Static,
            pricing: self
                .catalog_entry()
                .and_then(|m| m.pricing.clone())
                .or_else(|| Some(crate::pricing::volcengine_pricing(&self.model))),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let thinking_enabled = options.thinking != ThinkingLevel::Off && self.supports_thinking();

        // Reasoning output exclusion is handled uniformly via the profile's
        // option_support declaration + shared CompatibilityPolicy logic.
        let (effective_thinking, exclusion_adjustment) =
            crate::protocol::resolve_reasoning_exclusion(
                self.profile,
                &self.cx(),
                options,
                thinking_enabled,
            )?;

        let (body, mut option_adjustments) =
            self.build_request_body(messages, tools, options, effective_thinking);
        option_adjustments.extend(exclusion_adjustment);

        let start = Instant::now();
        let response = crate::http::shared_client()
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("volcengine", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("volcengine".into()),
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
                        err.get("code").and_then(|v| v.as_str()).map(String::from),
                        err.get("message")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    )
                })
                .unwrap_or((None, None));

            telemetry::record_model_error("volcengine", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("volcengine".into()),
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
            Some("reasoning_content"),
            None,
            start,
        )
        .await
        .map_err(|mut e| {
            telemetry::record_model_error("volcengine", &self.model, start.elapsed());
            e.provider = Some("volcengine".into());
            e
        })?;

        let content = sse.content;
        let usage = sse.usage;
        let first_token_latency = sse.first_token_latency;

        let stop_reason = match &sse.stop_reason {
            StopReason::Other(raw) => map_stop_reason(raw),
            _ => sse.stop_reason,
        };

        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let duration = start.elapsed();
        telemetry::record_model_success(
            "volcengine",
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

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

// ADR-0002 protocol-factory path (slice 003). Referenced as data by the
// Volcengine `ProviderEntry.build_chat`; the factory never matches on provider
// name (ADR rule 1). Transitional wrapping; collapsed into the Chat core in v0.12.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_chat_adapter(
    config: &orchest_provider_core::registry::ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn orchest_protocol::ChatModel>, orchest_protocol::ProtocolError> {
    let adapter = VolcengineAdapter::from_config(VolcengineConfig {
        model: resolved.model.to_string(),
        max_tokens: config.max_tokens.unwrap_or(crate::defaults::MAX_TOKENS),
        api_key: config.api_key.clone(),
        api_url: config.api_url.clone(),
    })
    .map_err(orchest_protocol::ProtocolError::from)?;
    Ok(Box::new(adapter))
}

#[cfg(test)]
mod tests;
