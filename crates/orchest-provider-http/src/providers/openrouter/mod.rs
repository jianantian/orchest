//! OpenRouter adapter implementation (multi-provider routing).

mod request;

use std::env;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    CacheCapability, CapabilitySource, Message, ModelAdapter, ModelCapabilities, ModelError,
    ModelResponse, OptionAdjustment, ReasoningCapability, RequestOptions, StreamEvent,
    ThinkingLevel, ToolDef, UpstreamErrorDetail,
};

use crate::{defaults, telemetry};

use request::normalize_chat_url;

pub struct OpenRouterAdapter {
    pub(super) api_key: String,
    pub(super) api_url: String,
    pub(super) model: String,
    pub(super) max_tokens: u32,
    pub(super) app_title: Option<String>,
    pub(super) site_url: Option<String>,
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
    pub app_title: Option<String>,
    pub site_url: Option<String>,
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

        let app_title = config
            .app_title
            .or_else(|| env::var("OPENROUTER_APP_TITLE").ok());
        let site_url = config
            .site_url
            .or_else(|| env::var("OPENROUTER_SITE_URL").ok());

        Ok(Self {
            api_key,
            api_url: normalize_chat_url(&api_url),
            model: config.model,
            max_tokens: config.max_tokens,
            app_title,
            site_url,
        })
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

        if let Some(ref title) = self.app_title {
            request = request.header("X-OpenRouter-Title", title);
        }
        if let Some(ref url) = self.site_url {
            request = request.header("HTTP-Referer", url);
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
        let usage = sse.usage;
        let stop_reason = sse.stop_reason;
        let first_token_latency = sse.first_token_latency;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("openrouter", &self.model);
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

// ProviderFactory implementation

pub struct OpenRouterFactory;

impl crate::registry::ProviderFactory for OpenRouterFactory {
    fn provider_name(&self) -> &'static str {
        "openrouter"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = OpenRouterAdapter::from_config(OpenRouterConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
            app_title: None,
            site_url: None,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        crate::defaults::openrouter::API_KEY_ENV
    }
}

#[cfg(test)]
mod tests;
