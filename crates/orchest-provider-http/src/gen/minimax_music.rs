//! Minimax music generation as the spine [`GenTask`] (Issue 007). Ported from
//! `agent-runtime-aigc-providers`'s `music/minimax`, over the spine
//! [`ProtocolError`] / [`GenResult`].
//!
//! `POST {base}/v1/music_generation` (Bearer, JSON) is **synchronous**: it returns
//! `data.audio` inline and signals failure via a non-zero `base_resp.status_code`.
//! The spine path requests `output_format: url` so the asset is a URL; the result
//! is presented over [`SyncGenCache`].

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenHandle, GenRequest, GenResult,
    GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::{warn_unconsumed_params, SyncGenCache};
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "https://api.minimax.io";
const DEFAULT_MODEL: &str = "music-2.6";

/// Minimax music gen-task configuration.
#[derive(Debug, Clone)]
pub struct MinimaxMusicConfig {
    pub model: String,
    pub api_key: String,
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Minimax music HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Minimax music HTTP {code}: {body}"),
    )
    .with_status(code)
}

/// [`GenRequest::params`] keys the Minimax music API understands
/// (docs/external/minimax/music/generation.md). `model`/`prompt` are set
/// explicitly and skipped in the passthrough; anything outside this set warns
/// via [`warn_unconsumed_params`] — it would be forwarded verbatim but have no
/// effect on the API.
const CONSUMED_PARAMS: &[&str] = &[
    "lyrics",
    "audio_setting",
    "aigc_watermark",
    "is_instrumental",
    "lyrics_optimizer",
    "audio_url",
    "audio_base64",
    "cover_feature_id",
];

/// Build the `/v1/music_generation` body: `model` + `prompt`, `output_format:
/// url`, and any [`GenRequest::params`] passthrough (`lyrics`, `audio_setting`,
/// `aigc_watermark`, `is_instrumental`, `lyrics_optimizer`, `audio_url`,
/// `audio_base64`, `cover_feature_id` for `music-cover`, …). `output_format`
/// is forced to `url`. Typed [`MusicParams`](orchest_protocol::MusicParams)
/// `lyrics`/`instrumental` (mapped onto the API's `is_instrumental` spelling)
/// take precedence over the raw keys.
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    warn_unconsumed_params("minimax", CONSUMED_PARAMS, &request.params);
    let mut body = json!({ "model": model, "prompt": request.prompt });
    if let Some(params) = request.params.as_object() {
        for (key, value) in params {
            if key != "model" && key != "prompt" {
                body[key] = value.clone();
            }
        }
    }
    if let Some(music) = request.music.as_ref() {
        if let Some(lyrics) = music.lyrics.as_ref() {
            body["lyrics"] = json!(lyrics);
        }
        if let Some(instrumental) = music.instrumental {
            body["is_instrumental"] = json!(instrumental);
        }
    }
    body["output_format"] = json!("url");
    body
}

/// Project a completed `music_generation` response onto the spine result. A
/// non-zero `base_resp.status_code` is a provider task failure; `data.audio` is
/// the produced track URL.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_result(response: &Value) -> Result<GenResult, ProtocolError> {
    let status_code = response
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if status_code != 0 {
        let message = response
            .pointer("/base_resp/status_msg")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Minimax music status_code={status_code}"));
        return Err(ProtocolError::new(ErrorCode::ProviderTaskFailed, message));
    }
    let assets = response
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|url| {
            vec![GenAsset::Url {
                url: url.to_string(),
                media_type: Some("audio/mpeg".to_string()),
            }]
        })
        .unwrap_or_default();
    let diagnostic_metadata = json!({
        "provider": "minimax",
        "trace_id": response.get("trace_id").and_then(Value::as_str).unwrap_or(""),
        "extra_info": response.get("extra_info").cloned().unwrap_or(Value::Null),
    });
    Ok(GenResult {
        assets,
        diagnostic_metadata,
        timed_text: None,
    })
}

/// The Minimax music gen provider as the spine [`GenTask`].
pub struct MinimaxMusicGen {
    config: MinimaxMusicConfig,
    cache: SyncGenCache,
}

impl MinimaxMusicGen {
    pub fn new(config: MinimaxMusicConfig) -> Self {
        Self {
            config,
            cache: SyncGenCache::default(),
        }
    }
}

/// The static descriptor the registry filters on for the minimax music dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("minimax", DEFAULT_MODEL, Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Audio])
}

/// Build a [`MinimaxMusicGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<MinimaxMusicGen, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "minimax music requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        cfg.model.clone()
    };
    let api_base_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(MinimaxMusicGen::new(MinimaxMusicConfig {
        model,
        api_key,
        api_base_url,
    }))
}

#[async_trait]
impl GenTask for MinimaxMusicGen {
    fn provider_name(&self) -> &str {
        "minimax"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("minimax", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Audio])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = crate::http::shared_client()
            .post(format!("{}/v1/music_generation", self.config.api_base_url))
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
                format!("failed to parse Minimax music response: {e}"),
            )
        })?;
        let result = build_result(&value)?;
        Ok(self.cache.store("minimax", result))
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
    use orchest_protocol::MusicParams;

    fn request(prompt: &str, params: Value) -> GenRequest {
        GenRequest {
            prompt: prompt.to_string(),
            params,
            music: None,
        }
    }

    fn request_with_music(prompt: &str, params: Value, music: MusicParams) -> GenRequest {
        GenRequest {
            prompt: prompt.to_string(),
            params,
            music: Some(music),
        }
    }

    #[test]
    fn submit_body_typed_fields_win_over_raw() {
        let body = build_submit_body(
            "music-2.6",
            &request_with_music(
                "lofi beat",
                json!({"lyrics": "raw lyrics", "is_instrumental": false}),
                MusicParams {
                    lyrics: Some("typed lyrics".to_string()),
                    instrumental: Some(true),
                    ..MusicParams::default()
                },
            ),
        );
        assert_eq!(body["lyrics"], "typed lyrics");
        assert_eq!(body["is_instrumental"], true);
        assert_eq!(body["output_format"], "url");
    }

    #[test]
    fn submit_body_sets_model_prompt_and_forces_url_format() {
        let body = build_submit_body(
            "music-2.6",
            &request(
                "lofi beat",
                json!({"lyrics": "la la", "is_instrumental": true, "output_format": "hex"}),
            ),
        );
        assert_eq!(body["model"], "music-2.6");
        assert_eq!(body["prompt"], "lofi beat");
        assert_eq!(body["lyrics"], "la la");
        assert_eq!(body["is_instrumental"], true);
        // output_format is forced to url even if params asked for hex
        assert_eq!(body["output_format"], "url");
    }

    #[test]
    fn submit_body_passes_through_cover_params() {
        let body = build_submit_body(
            "music-cover",
            &request(
                "upbeat synthwave cover",
                json!({"audio_url": "https://ref/track.mp3", "lyrics": "[verse]\ncover lyrics"}),
            ),
        );
        assert_eq!(body["model"], "music-cover");
        assert_eq!(body["audio_url"], "https://ref/track.mp3");
        assert_eq!(body["lyrics"], "[verse]\ncover lyrics");
        assert_eq!(body["output_format"], "url");
    }

    #[test]
    fn build_result_extracts_extra_info_into_metadata() {
        let result = build_result(&json!({
            "base_resp": {"status_code": 0, "status_msg": "success"},
            "data": {"audio": "https://m/track.mp3"},
            "trace_id": "abc123",
            "extra_info": {"music_duration": 25364, "music_sample_rate": 44100}
        }))
        .unwrap();
        assert_eq!(result.diagnostic_metadata["trace_id"], "abc123");
        assert_eq!(
            result.diagnostic_metadata["extra_info"]["music_duration"],
            25364
        );
    }

    #[test]
    fn build_result_extracts_audio_url() {
        let result = build_result(&json!({
            "base_resp": {"status_code": 0, "status_msg": "success"},
            "data": {"audio": "https://m/track.mp3"}
        }))
        .unwrap();
        assert_eq!(
            result.assets,
            vec![GenAsset::Url {
                url: "https://m/track.mp3".to_string(),
                media_type: Some("audio/mpeg".to_string()),
            }]
        );
    }

    #[test]
    fn build_result_surfaces_base_resp_error() {
        let err = build_result(&json!({
            "base_resp": {"status_code": 1004, "status_msg": "auth failed"}
        }))
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ProviderTaskFailed);
        assert!(err.message.contains("auth failed"));
    }
}
