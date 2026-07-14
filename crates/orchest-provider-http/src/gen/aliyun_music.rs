//! Aliyun fun-music generation as the spine [`GenTask`]. Ported over the spine
//! [`ProtocolError`] / [`GenResult`].
//!
//! Aliyun fun-music is **synchronous**: `POST {api_base_url}` returns the
//! completed audio URL inline in the same response, with no job id to poll.
//! The result is cached via [`SyncGenCache`] so the submit -> poll -> fetch
//! lifecycle is honored without re-generating on `fetch`.
//!
//! Auth: `Bearer $DASHSCOPE_API_KEY`. Defaults live in
//! [`crate::defaults::aliyun_music`].

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenHandle, GenRequest, GenResult,
    GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::SyncGenCache;
use serde_json::{json, Value};

use crate::defaults::aliyun_music;

/// Aliyun fun-music gen-task configuration.
#[derive(Debug, Clone)]
pub struct AliyunMusicConfig {
    pub model: String,
    pub api_key: String,
    /// Full generation endpoint URL (base + path), since
    /// [`aliyun_music::API_URL`] includes `/api/v1/services/audio/music/generation`.
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Aliyun music HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Aliyun music HTTP {code}: {body}"),
    )
    .with_status(code)
}

/// Build the DashScope music generation body: the envelope `{ model, input: {
/// prompt, format, is_instrumental, gender } }`, then any
/// [`GenRequest::params`] passthrough (`lyrics`, `gender`, `is_instrumental`,
/// `format`, `enable_aigc_watermark`, …) overrides the defaults. The `model`
/// and `prompt` keys are skipped when iterating params (they are set
/// explicitly).
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    let mut input = json!({
        "prompt": request.prompt,
        "format": "mp3",
        "is_instrumental": false,
        "gender": "female",
    });
    if let Some(params) = request.params.as_object() {
        for (key, value) in params {
            if key != "model" && key != "prompt" {
                input[key] = value.clone();
            }
        }
    }
    json!({ "model": model, "input": input })
}

/// Project a completed DashScope music generation response onto the spine
/// result. The audio URL is at `/output/audio/url`; an empty or missing URL
/// yields empty assets (not an error). `diagnostic_metadata` carries
/// `provider`, `request_id`, `usage`, and `extra_info`.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_result(response: &Value) -> Result<GenResult, ProtocolError> {
    let url = response
        .pointer("/output/audio/url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let assets = if url.is_empty() {
        Vec::new()
    } else {
        vec![GenAsset::Url {
            url: url.to_string(),
            media_type: Some("audio/mpeg".to_string()),
        }]
    };
    let diagnostic_metadata = json!({
        "provider": "aliyun",
        "request_id": response.get("request_id").and_then(Value::as_str).unwrap_or(""),
        "usage": response.get("usage").cloned().unwrap_or(Value::Null),
        "extra_info": response.get("extra_info").cloned().unwrap_or(Value::Null),
    });
    Ok(GenResult {
        assets,
        diagnostic_metadata,
        lrc: None,
    })
}

/// The Aliyun fun-music gen provider as the spine [`GenTask`].
pub struct AliyunMusicGen {
    config: AliyunMusicConfig,
    cache: SyncGenCache,
}

impl AliyunMusicGen {
    pub fn new(config: AliyunMusicConfig) -> Self {
        Self {
            config,
            cache: SyncGenCache::default(),
        }
    }
}

/// The static descriptor the registry filters on for the aliyun music dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("aliyun", aliyun_music::DEFAULT_MODEL, Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Audio])
}

/// Build an [`AliyunMusicGen`] from a registry [`ProviderConfig`]. The API key
/// is read from `cfg.api_key` or the `DASHSCOPE_API_KEY` env var; the endpoint
/// URL from `cfg.api_url` or `DASHSCOPE_API_URL` or the default; `model` from
/// `cfg.model` or the default `fun-music-v1`. A trailing `/` on the URL is
/// trimmed.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<AliyunMusicGen, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .or_else(|| {
            std::env::var(aliyun_music::API_KEY_ENV)
                .ok()
                .filter(|s| !s.is_empty())
        })
        .ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::MissingApiKey,
                "aliyun music requires api_key (set DASHSCOPE_API_KEY)",
            )
        })?;
    let model = if cfg.model.is_empty() {
        aliyun_music::DEFAULT_MODEL.to_string()
    } else {
        cfg.model.clone()
    };
    let api_base_url = cfg
        .api_url
        .clone()
        .or_else(|| {
            std::env::var(aliyun_music::API_URL_ENV)
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| aliyun_music::API_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(AliyunMusicGen::new(AliyunMusicConfig {
        model,
        api_key,
        api_base_url,
    }))
}

#[async_trait]
impl GenTask for AliyunMusicGen {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("aliyun", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Audio])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = crate::http::shared_client()
            .post(&self.config.api_base_url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            ProtocolError::new(
                ErrorCode::ProviderHttpError,
                format!("failed to parse Aliyun music response: {e}"),
            )
        })?;
        let result = parse_result(&value)?;
        Ok(self.cache.store("aliyun", result))
    }

    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
        Ok(self.cache.status(handle))
    }

    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        self.cache.fetch(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(prompt: &str, params: Value) -> GenRequest {
        GenRequest {
            prompt: prompt.to_string(),
            params,
        }
    }

    #[test]
    fn submit_body_sets_model_prompt_and_defaults() {
        let body = build_submit_body("fun-music-v1", &request("lofi beat", json!({})));
        assert_eq!(body["model"], "fun-music-v1");
        assert_eq!(body["input"]["prompt"], "lofi beat");
        assert_eq!(body["input"]["format"], "mp3");
        assert_eq!(body["input"]["is_instrumental"], false);
        assert_eq!(body["input"]["gender"], "female");
    }

    #[test]
    fn submit_body_overrides_input_from_params() {
        let body = build_submit_body(
            "fun-music-v1",
            &request(
                "upbeat synthwave",
                json!({
                    "lyrics": "[verse]\nneon lights",
                    "gender": "male",
                    "is_instrumental": true,
                    "format": "wav",
                    "enable_aigc_watermark": true,
                }),
            ),
        );
        assert_eq!(body["model"], "fun-music-v1");
        assert_eq!(body["input"]["prompt"], "upbeat synthwave");
        assert_eq!(body["input"]["lyrics"], "[verse]\nneon lights");
        assert_eq!(body["input"]["gender"], "male");
        assert_eq!(body["input"]["is_instrumental"], true);
        assert_eq!(body["input"]["format"], "wav");
        assert_eq!(body["input"]["enable_aigc_watermark"], true);
    }

    #[test]
    fn submit_body_skips_model_and_prompt_keys_in_params() {
        let body = build_submit_body(
            "fun-music-v1",
            &request(
                "real prompt",
                json!({"model": "should-be-ignored", "prompt": "should-be-ignored"}),
            ),
        );
        assert_eq!(body["model"], "fun-music-v1");
        assert_eq!(body["input"]["prompt"], "real prompt");
    }

    #[test]
    fn parse_result_extracts_audio_url() {
        let result = parse_result(&json!({
            "output": {"audio": {"url": "https://dashscope/track.mp3"}},
            "request_id": "req-123",
            "usage": {"duration": 120},
        }))
        .unwrap();
        assert_eq!(
            result.assets,
            vec![GenAsset::Url {
                url: "https://dashscope/track.mp3".to_string(),
                media_type: Some("audio/mpeg".to_string()),
            }]
        );
    }

    #[test]
    fn parse_result_includes_diagnostic_metadata() {
        let result = parse_result(&json!({
            "output": {"audio": {"url": "https://dashscope/track.mp3"}},
            "request_id": "req-abc",
            "usage": {"duration": 90},
            "extra_info": {"sample_rate": 44100}
        }))
        .unwrap();
        assert_eq!(result.diagnostic_metadata["provider"], "aliyun");
        assert_eq!(result.diagnostic_metadata["request_id"], "req-abc");
        assert_eq!(result.diagnostic_metadata["usage"]["duration"], 90);
        assert_eq!(
            result.diagnostic_metadata["extra_info"]["sample_rate"],
            44100
        );
    }

    #[test]
    fn parse_result_empty_url_yields_empty_assets() {
        let result = parse_result(&json!({
            "output": {"audio": {"url": ""}},
            "request_id": "req-empty"
        }))
        .unwrap();
        assert!(result.assets.is_empty());
    }

    #[test]
    fn parse_result_missing_audio_yields_empty_assets() {
        let result = parse_result(&json!({"request_id": "req-missing"})).unwrap();
        assert!(result.assets.is_empty());
        assert_eq!(result.diagnostic_metadata["request_id"], "req-missing");
    }
}
