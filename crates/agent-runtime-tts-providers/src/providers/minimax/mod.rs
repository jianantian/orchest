//! Minimax TTS adapter — synchronous WSS (`/ws/v1/t2a_v2`) + async HTTP
//! (`/v1/t2a_async_v2`). The single [`MinimaxTtsAdapter`] dispatches to the
//! sync or async path based on `request.provider_options.operation`
//! (`"async"`); default is sync.
//!
//! Streaming surfaces: `stream_synthesize` / `start_duplex_stream` are
//! deliberately NOT implemented in this iteration — they return
//! `UnsupportedOperation` so callers don't get a silently-empty stream.
//! `capabilities()` reports `single_streaming: false` / `duplex_streaming:
//! false` to keep the router honest. The WSS frame primitives in
//! [`sync`] are wired and unit-tested; a future PR can flip the bools and
//! plug them into a real I/O loop.
//!
//! `VoiceManager` impl lives in [`voice`] (issue 005); `list_voices` here
//! returns `UnsupportedOperation` until a dedicated `/voice_list` endpoint
//! lands in Minimax docs.
//!
//! [`files`] (multipart upload + retrieve) is exposed for external callers
//! preparing `text_file_id` (long-form async TTS input) or `file_id` (Voice
//! Clone prompt audio). It is not internally consumed by `synthesize`.
//!
//! Design: spec §4 / research §3.

pub(crate) mod async_http;
pub mod files;
pub(crate) mod protocol;
pub(crate) mod sync;
pub(crate) mod voice;

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{TtsError, TtsErrorCode};
use crate::streaming::{TtsDuplexStream, TtsOutputStream};
use crate::traits::TtsProvider;
use crate::types::{
    AudioData, AudioFormat, DuplexSynthesizeRequest, ListVoicesRequest, SynthesizeRequest,
    SynthesizeResult, TtsInput, TtsInputKind, TtsModelCapabilities, TtsUsage, VoiceInfo, VoiceKind,
};

const DEFAULT_HTTP_URL: &str = "https://api.minimaxi.com";
const DEFAULT_WSS_URL: &str = "wss://api.minimaxi.com/ws/v1/t2a_v2";

#[derive(Debug, Clone)]
pub struct MinimaxTtsConfig {
    pub model: String,
    pub api_key: String,
    /// Override HTTPS base URL (default `https://api.minimaxi.com`). The async
    /// path appends `/v1/t2a_async_v2`; files upload appends `/v1/files/...`.
    pub api_url: Option<String>,
    /// Override the synchronous WSS endpoint. Default
    /// `wss://api.minimaxi.com/ws/v1/t2a_v2`.
    pub wss_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct MinimaxTtsAdapter {
    config: MinimaxTtsConfig,
    http_url: String,
    wss_url: String,
    http_client: reqwest::Client,
}

impl std::fmt::Debug for MinimaxTtsAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinimaxTtsAdapter")
            .field("model", &self.config.model)
            .field("http_url", &self.http_url)
            .field("wss_url", &self.wss_url)
            .finish()
    }
}

impl MinimaxTtsAdapter {
    pub fn from_config(config: MinimaxTtsConfig) -> Result<Self, TtsError> {
        if config.model.trim().is_empty() {
            return Err(TtsError::new(
                TtsErrorCode::UnknownModel,
                "model cannot be empty",
            ));
        }
        if config.api_key.trim().is_empty() {
            return Err(TtsError::new(
                TtsErrorCode::MissingApiKey,
                "API key cannot be empty",
            ));
        }
        let http_url = config
            .api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_HTTP_URL.into())
            .trim_end_matches('/')
            .to_string();
        let wss_url = config
            .wss_url
            .clone()
            .unwrap_or_else(|| DEFAULT_WSS_URL.into());
        let mut builder = reqwest::Client::builder();
        if let Some(t) = config.timeout {
            builder = builder.timeout(t);
        }
        let http_client = builder.build().map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderHttpError,
                format!("reqwest client build failed: {err}"),
            )
        })?;
        Ok(Self {
            config,
            http_url,
            wss_url,
            http_client,
        })
    }
    pub(crate) fn api_key(&self) -> &str {
        &self.config.api_key
    }

    /// Raw model name (for VoiceManager response synthesis).
    pub(crate) fn model_name_raw(&self) -> &str {
        &self.config.model
    }

    /// Common JSON POST helper for VoiceManager endpoints. Returns the raw
    /// response body; callers parse + map `base_resp` themselves.
    pub(crate) async fn post_json(&self, path: &str, body: &Value) -> Result<String, TtsError> {
        use crate::error::TtsErrorCode;
        let url = format!("{}{}", self.http_url, path);
        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(body)
            .send()
            .await
            .map_err(|err| {
                TtsError::new(
                    TtsErrorCode::ProviderHttpError,
                    format!("Minimax {path} request failed: {err}"),
                )
            })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(TtsError::new(
                TtsErrorCode::ProviderHttpError,
                format!("Minimax {path} HTTP {status}: {text}"),
            )
            .with_upstream(Some(status.as_u16()), None, None, None));
        }
        Ok(text)
    }

    /// Dispatch: `provider_options.operation == "async"` selects the HTTP
    /// async path; everything else uses the synchronous WSS path. Spec §4d.
    fn wants_async(opts: &Value) -> bool {
        opts.get("operation")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s == "async")
    }
}

#[async_trait]
impl TtsProvider for MinimaxTtsAdapter {
    fn provider_name(&self) -> &str {
        "minimax"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> TtsModelCapabilities {
        TtsModelCapabilities {
            batch_synthesis: true,
            // Streaming surfaces are not implemented this iteration; the
            // router relies on these bools to refuse to route Stream/Duplex
            // operations here. Flip + plug a real I/O loop in a future PR.
            single_streaming: false,
            duplex_streaming: false,
            async_synthesis: true,
            input_kinds: vec![TtsInputKind::Text, TtsInputKind::Ssml],
            languages: Vec::new(),
            voice_kinds: vec![VoiceKind::System, VoiceKind::Cloned, VoiceKind::Designed],
            batch_output_formats: vec![
                AudioFormat::Mp3,
                AudioFormat::Pcm16Le,
                AudioFormat::WavPcm16Le,
            ],
            // Populated for when streaming is enabled — keeps the contract
            // ready without committing the I/O loop yet.
            stream_output_formats: vec![AudioFormat::Mp3, AudioFormat::Pcm16Le],
            supports_instruction: false,
            supports_emotion: true,
            supports_style: false,
            supports_ssml: true,
        }
    }

    async fn synthesize(&self, request: SynthesizeRequest) -> Result<SynthesizeResult, TtsError> {
        let text = match &request.input {
            TtsInput::Text(t) => t.clone(),
            TtsInput::Ssml(s) => s.clone(),
        };
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| self.config.model.clone());
        let trace_id = request
            .trace_id
            .clone()
            .unwrap_or_else(|| format!("minimax-{}", uuid::Uuid::new_v4()));
        let usage = TtsUsage::for_text(&text, None);
        let telemetry = crate::observability::TtsTelemetryBuilder::new(
            trace_id,
            "minimax",
            &model,
            crate::types::TtsOperation::Batch,
        )
        .input_chars(usage.input_chars)
        .build();

        if Self::wants_async(&request.provider_options) {
            // Async HTTP path: POST /v1/t2a_async_v2 → return AudioData::Url.
            let body = async_http::build_async_request(
                &model,
                &request.voice,
                &request.output.format,
                request.output.sample_rate_hz,
                &request.controls,
                Some(&text),
                request
                    .provider_options
                    .get("text_file_id")
                    .and_then(|v| v.as_u64()),
            );
            let resp = async_http::run_async_request(
                &self.http_client,
                &self.http_url,
                self.api_key(),
                body,
            )
            .await?;
            // Async returns file_id; surface as an opaque URL handle that
            // callers can resolve via /v1/files/retrieve. We surface the
            // canonical retrieve URL — spec §4d "是否预下载留给上层".
            let file_id = resp.file_id.ok_or_else(|| {
                TtsError::new(
                    TtsErrorCode::ProviderTaskFailed,
                    "Minimax async response missing file_id",
                )
            })?;
            let url = format!("{}/v1/files/retrieve?file_id={}", self.http_url, file_id);
            return Ok(SynthesizeResult {
                audio: AudioData::Url {
                    url,
                    expires_at: None,
                },
                format: request.output.format.clone(),
                duration_ms: None,
                usage,
                option_adjustments: Vec::new(),
                provider_metadata: serde_json::json!({
                    "task_id": resp.task_id,
                    "task_token": resp.task_token,
                    "file_id": file_id,
                }),
                telemetry,
            });
        }

        // Sync WSS path.
        let start_body = sync::build_task_start_body(
            &model,
            &request.voice,
            &request.output.format,
            request.output.sample_rate_hz,
            &request.controls,
        );
        let outcome =
            sync::run_sync_session(&self.wss_url, self.api_key(), start_body, &text).await?;
        Ok(SynthesizeResult {
            audio: AudioData::Bytes(outcome.audio),
            format: request.output.format.clone(),
            duration_ms: None,
            usage,
            option_adjustments: Vec::new(),
            provider_metadata: outcome.extra_info.unwrap_or(Value::Null),
            telemetry,
        })
    }

    async fn stream_synthesize(
        &self,
        _request: SynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError> {
        // Frame-level primitives in sync.rs are wired and unit-tested, but
        // the I/O loop that pushes per-frame chunks to a `TtsOutputStream`
        // is not implemented this iteration. capabilities().single_streaming
        // is `false`, so router-driven callers won't reach here.
        Err(TtsError::unsupported_operation())
    }

    async fn start_duplex_stream(
        &self,
        _request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError> {
        // Same status as `stream_synthesize` — pure-function primitives are
        // ready, the I/O loop isn't. capabilities().duplex_streaming = false
        // keeps the router from picking this provider for Duplex.
        Err(TtsError::unsupported_operation())
    }

    async fn list_voices(&self, _request: ListVoicesRequest) -> Result<Vec<VoiceInfo>, TtsError> {
        // Voice listing lives behind `VoiceManager` (issue 005). Until that
        // ships, return UnsupportedOperation rather than silently returning
        // an empty list.
        Err(TtsError::unsupported_operation())
    }
}
