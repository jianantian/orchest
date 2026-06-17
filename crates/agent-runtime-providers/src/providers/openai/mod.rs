//! OpenAI adapter implementation.
//!
//! Split by concern: this file owns the adapter struct, capability
//! reporting, and `complete()`'s control flow; [`request`] builds the
//! chat completions request body.

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
    RequestOptions, StreamEvent, ThinkingLevel, ToolDef, UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

use request::normalize_chat_url;

pub struct OpenAiAdapter {
    pub(super) api_key: String,
    pub(super) api_url: String,
    pub(super) model: String,
    pub(super) max_tokens: u32,
}

impl std::fmt::Debug for OpenAiAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct OpenAiConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl OpenAiAdapter {
    pub fn from_config(config: OpenAiConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "OPENAI_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .or_else(|| env::var("OPENAI_API_URL").ok())
            .or_else(|| env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_else(|| defaults::openai::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "OpenAI API URL cannot be empty",
                "invalid_api_url",
            ));
        }

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config
                .model
                .strip_prefix("openai/")
                .unwrap_or(&config.model)
                .to_string(),
            max_tokens: config.max_tokens,
        })
    }

    fn supports_reasoning(&self) -> bool {
        request::supports_reasoning_model(&self.model)
    }
}

#[async_trait]
impl ModelAdapter for OpenAiAdapter {
    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        let supports_reasoning = self.supports_reasoning();

        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: supports_reasoning,
                efforts: if supports_reasoning {
                    vec![
                        ThinkingLevel::Low,
                        ThinkingLevel::Medium,
                        ThinkingLevel::High,
                    ]
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
            context_window_size: Some(request::openai_context_window(&self.model)),
            source: CapabilitySource::Static,
            pricing: Some(crate::pricing::openai_pricing(&self.model)),
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let _span = telemetry::model_complete_span("openai", &self.model, tx.is_some());

        if options.compatibility_policy == CompatibilityPolicy::Strict
            && options.thinking_budget_tokens.is_some()
        {
            return Err(ModelError::internal(
                "thinking_budget_tokens is not supported by OpenAI",
                "unsupported_thinking_budget",
            ));
        }
        if options.compatibility_policy == CompatibilityPolicy::Strict
            && options.thinking != ThinkingLevel::Off
            && !self.supports_reasoning()
        {
            return Err(ModelError::internal(
                format!(
                    "model '{}' does not declare OpenAI reasoning support",
                    self.model
                ),
                "unsupported_reasoning_model",
            ));
        }

        let (body, mut option_adjustments) = self.build_request_body(messages, tools, options);

        let start = Instant::now();
        let response = crate::http::shared_client()
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("openai", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("openai".into()),
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

            telemetry::record_model_error("openai", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("openai".into()),
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

        let sse = crate::sse::parse_openai_sse_stream(stream, tx_ref, None, None, start)
            .await
            .map_err(|mut e| {
                telemetry::record_model_error("openai", &self.model, start.elapsed());
                e.provider = Some("openai".into());
                e
            })?;

        let content = sse.content;
        let usage = sse.usage;
        let stop_reason = sse.stop_reason;
        let first_token_latency = sse.first_token_latency;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("openai", &self.model);
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
            "openai",
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

pub struct OpenAiFactory;

impl crate::registry::ProviderFactory for OpenAiFactory {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        crate::defaults::openai::API_KEY_ENV
    }
}

#[cfg(test)]
mod tests;
