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

// ProviderFactory implementation

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
#[allow(dead_code)]
pub(crate) mod test_util {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub async fn serve_sse_once(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 8192];
            let _ = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("test server should write response");
        });

        format!("http://{address}")
    }

    pub async fn serve_sse_once_capture(
        body: &'static str,
    ) -> (String, tokio::sync::oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        let (capture_tx, capture_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 16384];
            let n = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            let req_str = String::from_utf8_lossy(&request[..n]).to_string();
            let _ = capture_tx.send(req_str);

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("test server should write response");
        });

        (format!("http://{address}"), capture_rx)
    }

    pub async fn serve_status(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 8192];
            let _ = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            let response = format!(
                "HTTP/1.1 {status} Error\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("test server should write response");
        });

        format!("http://{address}")
    }

    pub async fn serve_partial_sse(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");

        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept one request");
            let mut request = vec![0; 8192];
            let _ = socket
                .read(&mut request)
                .await
                .expect("test server should read request");
            // No content-length, just write and drop the connection
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("test server should write response");
        });

        format!("http://{address}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::anthropic::test_util::*;
    use crate::{CachePolicy, ContentBlock, StopReason};
    use response::map_stop_reason;

    const MINIMAL_SSE: &str = r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#;

    fn default_options() -> RequestOptions {
        RequestOptions {
            thinking: ThinkingLevel::Off,
            ..Default::default()
        }
    }

    fn make_adapter(api_url: &str) -> AnthropicAdapter {
        AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url.into()),
        })
        .expect("adapter should be created")
    }

    #[test]
    fn uses_default_api_url() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: None,
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, defaults::anthropic::API_URL);
    }

    #[test]
    fn uses_custom_api_url() {
        let api_url = "https://compatible.example.com/v1/messages";
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(api_url.into()),
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, api_url);
    }

    #[test]
    fn appends_messages_endpoint() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("https://openrouter.ai/api".into()),
        })
        .expect("adapter should be created");

        assert_eq!(adapter.api_url, "https://openrouter.ai/api/v1/messages");
    }

    #[test]
    fn rejects_empty_api_url() {
        let result = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some(" ".into()),
        });

        let error = result.expect_err("empty api url should be rejected");
        assert_eq!(error.code.as_deref(), Some("invalid_api_url"));
    }

    #[tokio::test]
    async fn stream_thinking_boundaries() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":3}}}

event: content_block_start
data: {"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"first "}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"second"}}

event: content_block_stop
data: {"content_block":{"signature":"sig-abc-123"}}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let (tx, mut rx) = mpsc::channel(16);
        let response = adapter
            .complete(&[], &[], &default_options(), Some(tx))
            .await
            .expect("should parse");

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        assert!(matches!(events[0], StreamEvent::ThinkingStart));
        assert!(matches!(&events[1], StreamEvent::Thinking { delta } if delta == "first "));
        assert!(matches!(&events[2], StreamEvent::Thinking { delta } if delta == "second"));
        assert!(
            matches!(&events[3], StreamEvent::ThinkingEnd { ref signature, .. } if signature.as_deref() == Some("sig-abc-123"))
        );
        assert!(matches!(events[4], StreamEvent::Done { .. }));
        assert_eq!(response.usage.input_tokens, 3);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[tokio::test]
    async fn stream_rejects_malformed_sse() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":3}}}

event: content_block_delta
data: {NOT VALID JSON!!!}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let err = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect_err("malformed JSON should produce an error");

        assert_eq!(err.code.as_deref(), Some("invalid_json"));
        assert!(err.message.contains("malformed SSE JSON"));
        assert!(err.upstream.is_some());
    }

    #[tokio::test]
    async fn stream_thinking_to_content_block() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"delta":{"type":"thinking_delta","thinking":"reasoning here"}}

event: content_block_stop
data: {"content_block":{"signature":"my-sig"}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"answer"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");

        assert_eq!(response.content.len(), 2);
        match &response.content[0] {
            ContentBlock::Thinking {
                text,
                signature,
                provider_details,
            } => {
                assert_eq!(text.as_deref(), Some("reasoning here"));
                assert_eq!(signature.as_deref(), Some("my-sig"));
                assert!(provider_details.is_none());
            }
            _ => panic!("expected Thinking block"),
        }
        assert!(matches!(&response.content[1], ContentBlock::Text(t) if t == "answer"));
    }

    #[test]
    fn thinking_level_maps_to_budget() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-3-opus".into(), // old model, uses enabled mode
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32768);
    }

    #[test]
    fn thinking_budget_override() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-3-opus".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            thinking_budget_tokens: Some(8000),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["budget_tokens"], 8000);
    }

    #[test]
    fn thinking_budget_ignored_in_adaptive_reports_adjustment() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(), // adaptive model
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            thinking_budget_tokens: Some(8000),
            ..Default::default()
        };
        let (body, adjustments) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "high");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].reason, "unsupported_in_adaptive_thinking");
    }

    #[test]
    fn adaptive_uses_output_config_effort() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-opus-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        for (level, expected) in [
            (ThinkingLevel::Low, "low"),
            (ThinkingLevel::Medium, "medium"),
            (ThinkingLevel::High, "high"),
            (ThinkingLevel::XHigh, "xhigh"),
            (ThinkingLevel::Max, "max"),
        ] {
            let opts = RequestOptions {
                thinking: level,
                ..Default::default()
            };
            let (body, _) = adapter.build_request_body(&[], &[], &opts);
            assert_eq!(body["thinking"]["type"], "adaptive");
            assert_eq!(body["output_config"]["effort"], expected, "level {level:?}");
        }
    }

    #[test]
    fn include_thinking_false_maps_to_omitted() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            include_thinking: false,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["thinking"]["display"], "omitted");
    }

    #[test]
    fn cache_policy_auto_adds_top_level_cache_control() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            cache_policy: CachePolicy::Auto,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn cache_policy_long_sets_1h_ttl() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            cache_policy: CachePolicy::Long,
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["cache_control"]["ttl"], "1h");
    }

    #[tokio::test]
    async fn cache_usage_mapped_to_token_usage() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":100,"cache_read_input_tokens":50,"cache_creation_input_tokens":20}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");

        assert_eq!(response.usage.input_tokens, 100);
        assert_eq!(response.usage.cache_read_tokens, 50);
        assert_eq!(response.usage.cache_write_tokens, 20);
    }

    #[test]
    fn temperature_forwarded() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            temperature: Some(0.7),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert!(
            body["temperature"].as_f64().unwrap() > 0.69
                && body["temperature"].as_f64().unwrap() < 0.71
        );
    }

    #[test]
    fn adaptive_vs_enabled_mode() {
        let adaptive = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();
        assert!(adaptive.supports_adaptive());

        let enabled = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-3-opus".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();
        assert!(!enabled.supports_adaptive());
    }

    #[tokio::test]
    async fn tool_use_start_and_end_emitted() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"tool_1","name":"read_file"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"/tmp\"}"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let (tx, mut rx) = mpsc::channel(16);
        let response = adapter
            .complete(&[], &[], &default_options(), Some(tx))
            .await
            .expect("should parse");

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        assert!(
            matches!(&events[0], StreamEvent::ToolUseStart { id, name } if id == "tool_1" && name == "read_file")
        );
        assert!(matches!(&events[1], StreamEvent::ToolUseArgsChunk { id, .. } if id == "tool_1"));
        assert!(matches!(&events[2], StreamEvent::ToolUseEnd { id } if id == "tool_1"));
        assert!(matches!(events[3], StreamEvent::Done { .. }));
        assert_eq!(response.stop_reason, StopReason::ToolUse);
    }

    #[tokio::test]
    async fn parallel_tool_uses_end_each() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"t1","name":"a"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{}"}}

event: content_block_stop
data: {}

event: content_block_start
data: {"content_block":{"type":"tool_use","id":"t2","name":"b"}}

event: content_block_delta
data: {"delta":{"type":"input_json_delta","partial_json":"{}"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let (tx, mut rx) = mpsc::channel(32);
        adapter
            .complete(&[], &[], &default_options(), Some(tx))
            .await
            .expect("should parse");

        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }

        let end_ids: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ToolUseEnd { id } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(end_ids, vec!["t1", "t2"]);
    }

    #[tokio::test]
    async fn tx_none_skips_events() {
        let api_url = serve_sse_once(MINIMAL_SSE).await;
        let adapter = make_adapter(&api_url);

        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should parse");

        assert_eq!(response.content.len(), 1);
        assert!(matches!(&response.content[0], ContentBlock::Text(t) if t == "Hello"));
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn max_tokens_override() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-test".into(),
            max_tokens: 128,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let opts = RequestOptions {
            thinking: ThinkingLevel::Off,
            max_tokens: Some(4096),
            ..Default::default()
        };
        let (body, _) = adapter.build_request_body(&[], &[], &opts);
        assert_eq!(body["max_tokens"], 4096);

        let opts_none = RequestOptions {
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            ..Default::default()
        };
        let (body2, _) = adapter.build_request_body(&[], &[], &opts_none);
        assert_eq!(body2["max_tokens"], 128);
    }

    #[tokio::test]
    async fn stream_interrupted_returns_error() {
        let api_url = serve_partial_sse(
            r#"event: message_start
data: {"message":{"usage":{"input_tokens":10}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"partial"}}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let err = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect_err("interrupted stream should error");

        assert_eq!(err.code.as_deref(), Some("stream_interrupted"));
    }

    #[tokio::test]
    async fn missing_usage_reports_adjustment() {
        let api_url = serve_sse_once(
            r#"event: message_start
data: {"message":{"usage":{}}}

event: content_block_start
data: {"content_block":{"type":"text"}}

event: content_block_delta
data: {"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {}

event: message_delta
data: {"delta":{"stop_reason":"end_turn"}}

event: message_stop
data: {}

"#,
        )
        .await;

        let adapter = make_adapter(&api_url);
        let response = adapter
            .complete(&[], &[], &default_options(), None)
            .await
            .expect("should succeed");

        let adj = response
            .option_adjustments
            .iter()
            .find(|a| a.reason == "usage_not_reported");
        assert!(adj.is_some(), "should report usage_not_reported adjustment");
    }

    #[tokio::test]
    async fn done_usage_matches_model_response() {
        let api_url = serve_sse_once(MINIMAL_SSE).await;
        let adapter = make_adapter(&api_url);

        let (tx, mut rx) = mpsc::channel(16);
        let response = adapter
            .complete(&[], &[], &default_options(), Some(tx))
            .await
            .expect("should parse");

        let mut done_usage = None;
        while let Some(e) = rx.recv().await {
            if let StreamEvent::Done { usage } = e {
                done_usage = Some(usage);
            }
        }

        let done_usage = done_usage.expect("should have Done event");
        assert_eq!(done_usage.input_tokens, response.usage.input_tokens);
        assert_eq!(done_usage.output_tokens, response.usage.output_tokens);
        assert!(
            response.usage.cost_usd.is_some(),
            "response should have cost_usd filled by adapter"
        );
    }

    #[test]
    fn provider_name_and_model_name() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        assert_eq!(adapter.provider_name(), "anthropic");
        assert_eq!(adapter.model_name(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn capabilities_reports_static_source() {
        let adapter = AnthropicAdapter::from_config(AnthropicConfig {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            api_key: Some("key".into()),
            api_url: Some("http://localhost".into()),
        })
        .unwrap();

        let caps = adapter.capabilities();
        assert!(caps.streaming);
        assert!(caps.tool_use);
        assert!(caps.reasoning.supported);
        assert!(caps.prompt_cache.supported);
        assert_eq!(caps.source, CapabilitySource::Static);
    }

    #[tokio::test]
    async fn stop_reason_mapping() {
        for (raw, expected) in [
            ("end_turn", StopReason::EndTurn),
            ("tool_use", StopReason::ToolUse),
            ("max_tokens", StopReason::MaxTokens),
            ("stop_sequence", StopReason::StopSequence),
            ("pause_turn", StopReason::Pause),
            ("compaction", StopReason::Pause),
            ("refusal", StopReason::Refusal),
            (
                "model_context_window_exceeded",
                StopReason::ContextWindowExceeded,
            ),
        ] {
            assert_eq!(map_stop_reason(raw), expected, "for {raw}");
        }
        assert!(
            matches!(map_stop_reason("unknown_reason"), StopReason::Other(s) if s == "unknown_reason")
        );
    }
}
