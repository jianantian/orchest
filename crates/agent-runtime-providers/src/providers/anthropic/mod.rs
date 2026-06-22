//! Anthropic Claude adapter implementation.
//!
//! Split by concern: this file owns the adapter struct, capability
//! reporting, and `complete()`'s control flow; [`request`] builds the
//! Messages API request body; [`response`] consumes the SSE response into a
//! normalized result.

mod request;
mod response;

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CapabilitySource, CompatibilityPolicy, Message, ModelAdapter,
    ModelCapabilities, ModelError, ModelPricing, ModelResponse, OptionAdjustment,
    ReasoningCapability, RequestOptions, StreamEvent, ThinkingLevel, ToolDef, UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

use request::normalize_messages_url;
use response::consume_event_stream;

pub struct AnthropicAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
}

impl std::fmt::Debug for AnthropicAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct AnthropicConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl AnthropicAdapter {
    pub fn from_config(config: AnthropicConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::anthropic::API_KEY_ENV).ok())
            .or_else(|| env::var(defaults::anthropic::AUTH_TOKEN_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .or_else(|| {
                env::var(defaults::anthropic::API_URL_ENV)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| defaults::anthropic::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "Anthropic API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        Ok(Self {
            api_key,
            api_url: normalize_messages_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
        })
    }

    fn supports_adaptive(&self) -> bool {
        let m = &self.model;
        // fable-5 and mythos-5: adaptive always-on
        // opus-4.x and sonnet-4.x: adaptive always-on
        // haiku-4.x: extended thinking only (no adaptive)
        m.starts_with("claude-fable-5")
            || m.starts_with("claude-mythos-5")
            || m.starts_with("claude-opus-4")
            || m.starts_with("claude-sonnet-4")
    }

    fn context_window_size(&self) -> u64 {
        let m = &self.model;
        if m.starts_with("claude-haiku-4") {
            200_000
        } else {
            1_000_000
        }
    }

    fn pricing(&self) -> ModelPricing {
        crate::pricing::anthropic_pricing(&self.model)
    }
}

#[async_trait]
impl ModelAdapter for AnthropicAdapter {
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let supports_adaptive = self.supports_adaptive();
        let efforts = if supports_adaptive {
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Minimal,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
                ThinkingLevel::Max,
            ]
        } else {
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ]
        };

        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts,
                budget_tokens: !supports_adaptive,
                output_exclusion: true,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: true,
                long_ttl: true,
            },
            max_output_tokens: Some(self.max_tokens),
            context_window_size: Some(self.context_window_size()),
            source: CapabilitySource::Static,
            pricing: Some(self.pricing()),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("anthropic", &self.model, tx.is_some());

        let (body, mut option_adjustments) = self.build_request_body(messages, tools, options);

        if options.compatibility_policy == CompatibilityPolicy::Strict
            && options.thinking_budget_tokens.is_some()
            && self.supports_adaptive()
            && options.thinking != ThinkingLevel::Off
        {
            return Err(ModelError::internal(
                "thinking_budget_tokens is not supported in adaptive thinking mode",
                "unsupported_thinking_budget",
            ));
        }

        let start = Instant::now();
        let response = crate::http::shared_client()
            .post(&self.api_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", defaults::anthropic::API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("anthropic", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("anthropic".into()),
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

            telemetry::record_model_error("anthropic", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("anthropic".into()),
                status: Some(status),
                retry_after_secs: None,
                upstream: Some(Arc::new(UpstreamErrorDetail {
                    code: upstream_code,
                    message: upstream_msg,
                    body: upstream_body,
                })),
            });
        }

        let outcome =
            consume_event_stream(response.bytes_stream(), "anthropic", tx.as_ref(), start)
                .await
                .map_err(|mut e| {
                    telemetry::record_model_error("anthropic", &self.model, start.elapsed());
                    e.provider.get_or_insert_with(|| "anthropic".into());
                    e
                })?;

        let mut usage = outcome.usage;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("anthropic", &self.model);
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
            "anthropic",
            &self.model,
            duration,
            usage.input_tokens,
            usage.output_tokens,
            outcome.first_token_latency,
            Some(duration),
        );

        usage.cost_usd = Some(self.pricing().calculate(&usage));

        Ok(ModelResponse {
            content: outcome.content,
            usage,
            stop_reason: outcome.stop_reason,
            option_adjustments,
        })
    }
}

pub struct AnthropicFactory;

impl crate::registry::ProviderFactory for AnthropicFactory {
    fn provider_name(&self) -> &'static str {
        "anthropic"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        defaults::anthropic::API_KEY_ENV
    }
}

#[cfg(test)]
pub(crate) mod test_util;

#[cfg(test)]
mod tests;
