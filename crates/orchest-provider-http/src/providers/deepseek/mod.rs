//! DeepSeek adapter implementation (OpenAI-compatible).

mod request;

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CapabilitySource, CompatibilityPolicy, Message, ModelAdapter,
    ModelCapabilities, ModelError, ModelResponse, OptionAdjustment, ReasoningCapability,
    RequestOptions, StopReason, StreamEvent, ThinkingLevel, ToolDef, UpstreamErrorDetail,
};

use crate::catalog::LlmModelEntry;
use crate::protocol::{Protocol, ProviderEntry, ProviderProfile, ResolvedModel};
use crate::{defaults, telemetry};

mod profile;
pub use profile::{DeepSeekProfile, DEEPSEEK_PROFILE};

pub struct DeepSeekAdapter {
    pub(super) api_key: String,
    pub(super) api_url: String,
    pub(super) model: String,
    pub(super) max_tokens: u32,
    /// ADR-0002 resolution context, used to build a [`ResolvedModel`] for the
    /// profile hooks. `entry` is always the DeepSeek entry; `catalog` is the
    /// model's catalog row (or `None` for unlisted models).
    pub(super) entry: &'static ProviderEntry,
    pub(super) catalog: Option<&'static LlmModelEntry>,
    /// The behavioral profile driving option lowering + reasoning replay.
    pub(super) profile: &'static dyn ProviderProfile,
}

impl std::fmt::Debug for DeepSeekAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepSeekAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct DeepSeekConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl DeepSeekAdapter {
    pub fn from_config(config: DeepSeekConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var("DEEPSEEK_API_KEY").ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "DEEPSEEK_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .unwrap_or_else(|| defaults::deepseek::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "DeepSeek API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        let entry = crate::protocol::provider_entry("deepseek")
            .expect("deepseek entry is registered on the protocol path");
        let catalog = crate::catalog::find_model(&config.model);
        let profile = entry
            .profile_for(Protocol::Chat)
            .expect("deepseek entry carries a Chat profile");

        Ok(Self {
            api_key,
            api_url: request::normalize_chat_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
            entry,
            catalog,
            profile,
        })
    }

    /// The ADR-0002 resolution context for this adapter, rebuilt per request so
    /// the profile hooks receive the provider entry, protocol, model, and catalog
    /// row without the adapter having to retain a borrowed `ResolvedModel`.
    pub(super) fn cx(&self) -> ResolvedModel<'_> {
        ResolvedModel {
            provider: self.entry,
            protocol: Protocol::Chat,
            model: &self.model,
            catalog: self.catalog,
        }
    }

    fn supports_thinking(&self) -> bool {
        // v4-flash and v4-pro both support thinking mode (see thinking_mode guide).
        // legacy deepseek-reasoner (deprecated 2026-07-24) also supports thinking.
        let m = &self.model;
        m.starts_with("deepseek-v4-flash")
            || m.starts_with("deepseek-v4-pro")
            || m.starts_with("deepseek-reasoner")
    }
}

#[async_trait]
impl ModelAdapter for DeepSeekAdapter {
    fn provider_name(&self) -> &str {
        "deepseek"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let context_window = if self.model.starts_with("deepseek-v4") {
            1_000_000
        } else {
            64_000
        };

        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: self.supports_thinking(),
                efforts: if self.supports_thinking() {
                    vec![ThinkingLevel::High, ThinkingLevel::Max]
                } else {
                    vec![]
                },
                budget_tokens: false,
                output_exclusion: false,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: Some(context_window),
            source: CapabilitySource::Static,
            pricing: Some(crate::pricing::deepseek_pricing(&self.model)),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("deepseek", &self.model, tx.is_some());
        let thinking_enabled = options.thinking != ThinkingLevel::Off;

        // Handle include_thinking: false when thinking is enabled
        let effective_thinking = if !options.include_thinking && thinking_enabled {
            match options.compatibility_policy {
                CompatibilityPolicy::Strict => {
                    return Err(ModelError {
                        message: "DeepSeek does not support output exclusion for reasoning".into(),
                        code: Some("unsupported_reasoning_output_exclusion".into()),
                        provider: Some("deepseek".into()),
                        status: None,
                        retry_after_secs: None,
                        upstream: None,
                    });
                }
                CompatibilityPolicy::Coerce => false,
            }
        } else {
            thinking_enabled
        };

        let (body, mut option_adjustments) =
            self.build_request_body(messages, tools, options, effective_thinking);

        if !options.include_thinking && thinking_enabled && !effective_thinking {
            option_adjustments.push(OptionAdjustment {
                option: "include_thinking".into(),
                requested: json!(false),
                applied: json!("thinking_disabled"),
                reason: "output_exclusion_unsupported_disables_reasoning".into(),
            });
        }

        let start = Instant::now();
        let response = crate::http::shared_client()
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("deepseek", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("deepseek".into()),
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

            telemetry::record_model_error("deepseek", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("deepseek".into()),
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

        let sse =
            crate::sse::parse_openai_sse_stream(stream, tx_ref, Some("reasoning"), None, start)
                .await
                .map_err(|mut e| {
                    telemetry::record_model_error("deepseek", &self.model, start.elapsed());
                    e.provider = Some("deepseek".into());
                    e
                })?;

        let content = sse.content;
        let mut usage = sse.usage;
        let stop_reason = sse.stop_reason;
        let first_token_latency = sse.first_token_latency;

        // Remap stop reason using DeepSeek-specific mapping
        let stop_reason = match &stop_reason {
            StopReason::Other(raw) => request::map_stop_reason(raw),
            _ => stop_reason,
        };

        usage.cache_write_tokens = 0;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("deepseek", &self.model);
            option_adjustments.push(OptionAdjustment {
                option: "usage".into(),
                requested: json!(null),
                applied: json!(null),
                reason: "usage_not_reported".into(),
            });
        }

        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let duration = start.elapsed();
        telemetry::record_model_success(
            "deepseek",
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

// ProviderFactory implementation

pub struct DeepSeekFactory;

impl crate::registry::ProviderFactory for DeepSeekFactory {
    fn provider_name(&self) -> &'static str {
        "deepseek"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = DeepSeekAdapter::from_config(DeepSeekConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        crate::defaults::deepseek::API_KEY_ENV
    }
}

// ADR-0002 protocol-factory path (slice 002). Referenced as data by the DeepSeek
// `ProviderEntry.build_chat`; `ChatProtocolFactory` calls it without matching on
// provider name (ADR rule 1). Transitional: wraps `DeepSeekAdapter`, whose
// request builder now sources its reasoning-dialect divergence from
// `DeepSeekProfile`. v0.12 collapses this into the Chat protocol core.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_chat_adapter(
    config: &orchest_provider_core::registry::ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn orchest_protocol::ChatModel>, orchest_protocol::ProtocolError> {
    let adapter = DeepSeekAdapter::from_config(DeepSeekConfig {
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
