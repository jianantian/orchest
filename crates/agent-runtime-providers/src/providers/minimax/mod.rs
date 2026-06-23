//! Minimax LLM adapter — Anthropic Messages API 兼容(`POST /anthropic/v1/messages`)。
//!
//! 设计来源:`docs/research/minimax-api-analysis.md` §二、迭代 v0.9.10 issue 002。
//! 协议与 [`crate::providers::anthropic`] 同形态(同路径 / 同 SSE 事件 / 同 `thinking`
//! schema / 同 tool 协议),因此请求构造与 SSE 解析直接 fork。差异点见 `request.rs`:
//! Image / Video / MidConvSystem 真实序列化、4 个 Minimax-only Role、`service_tier`。
//! 鉴权用 `Authorization: Bearer ${api_key}`(锚点 `llm/api.md:1354-1362`)。

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

pub struct MinimaxAdapter {
    api_key: String,
    api_url: String,
    model: String,
    max_tokens: u32,
}

impl std::fmt::Debug for MinimaxAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinimaxAdapter")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

pub struct MinimaxConfig {
    pub model: String,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl MinimaxAdapter {
    pub fn from_config(config: MinimaxConfig) -> Result<Self, ModelError> {
        let api_key = config
            .api_key
            .or_else(|| env::var(defaults::minimax::API_KEY_ENV).ok())
            .ok_or_else(|| {
                ModelError::internal(
                    "MINIMAX_API_KEY not set and no api_key provided",
                    "missing_api_key",
                )
            })?;

        let api_url = config
            .api_url
            .or_else(|| {
                env::var(defaults::minimax::API_URL_ENV)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| defaults::minimax::API_URL.to_string());

        if api_url.trim().is_empty() {
            return Err(ModelError::internal(
                "Minimax API URL cannot be empty",
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

    /// MiniMax-M3 是带 1M 上下文 + 原生多模态的 frontier 模型,支持 adaptive thinking;
    /// M2.x 系列 thinking 始终开启但无 adaptive(`llm/api.md:883`)。
    fn supports_adaptive(&self) -> bool {
        self.model.starts_with("MiniMax-M3")
    }

    fn context_window_size(&self) -> u64 {
        // `llm/desc.md:21-29` 模型表:M3 1M,M2 系列 204_800。
        if self.model.starts_with("MiniMax-M3") {
            1_000_000
        } else {
            204_800
        }
    }

    /// Minimax catalog 暂无定价数据;返回零成本占位(`docs/external/minimax/` 缺
    /// pricing.md,见研究文档 §八)。pricing 在 catalog 层定义后会替换为表驱动。
    fn pricing(&self) -> ModelPricing {
        ModelPricing {
            currency: "USD".into(),
            input_per_million: 0.0,
            output_per_million: 0.0,
            cache_read_per_million: None,
            cache_write_per_million: None,
        }
    }
}

#[async_trait]
impl ModelAdapter for MinimaxAdapter {
    fn provider_name(&self) -> &str {
        "minimax"
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
        let _span = telemetry::model_complete_span("minimax", &self.model, tx.is_some());

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
            .header("authorization", format!("Bearer {}", &self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                telemetry::record_model_error("minimax", &self.model, start.elapsed());
                ModelError {
                    message: e.to_string(),
                    code: Some("request_failed".into()),
                    provider: Some("minimax".into()),
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

            telemetry::record_model_error("minimax", &self.model, start.elapsed());
            return Err(ModelError {
                message: format!("API returned {status}: {body_text}"),
                code: Some(status.to_string()),
                provider: Some("minimax".into()),
                status: Some(status),
                retry_after_secs: None,
                upstream: Some(Arc::new(UpstreamErrorDetail {
                    code: upstream_code,
                    message: upstream_msg,
                    body: upstream_body,
                })),
            });
        }

        let outcome = consume_event_stream(response.bytes_stream(), "minimax", tx.as_ref(), start)
            .await
            .map_err(|mut e| {
                telemetry::record_model_error("minimax", &self.model, start.elapsed());
                e.provider.get_or_insert_with(|| "minimax".into());
                e
            })?;

        let mut usage = outcome.usage;

        let has_usage = usage.input_tokens > 0 || usage.output_tokens > 0;
        if !has_usage {
            telemetry::record_usage_missing("minimax", &self.model);
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
            "minimax",
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

pub struct MinimaxFactory;

impl crate::registry::ProviderFactory for MinimaxFactory {
    fn provider_name(&self) -> &'static str {
        "minimax"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let adapter = MinimaxAdapter::from_config(MinimaxConfig {
            model: model.to_string(),
            max_tokens,
            api_key: Some(api_key),
            api_url,
        })?;
        Ok(Box::new(adapter))
    }

    fn default_api_key_env(&self) -> &'static str {
        defaults::minimax::API_KEY_ENV
    }
}

#[cfg(test)]
mod tests;
