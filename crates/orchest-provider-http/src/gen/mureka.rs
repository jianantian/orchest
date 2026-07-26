//! Mureka music generation as the spine [`GenTask`] (Issue 007). Ported over
//! the spine [`ProtocolError`] / [`GenResult`].
//!
//! Mureka is **asynchronous**: `POST {base}/v1/song/generate` (or
//! `/v1/instrumental/generate`) returns a task `id` immediately; the client
//! polls `GET {base}/v1/song/query/{id}` (or the instrumental variant) until
//! `status` becomes `succeeded`/`failed`, then fetches `choices[].url`. The
//! job lifecycle is tracked in an in-memory `Mutex<HashMap<String, JobState>>`:
//! `submit` stores `Pending`, `poll` transitions `Pending -> Done(GenResult)`,
//! and `fetch` returns and consumes the stored result.
//!
//! Auth: `Bearer $MUREKA_API_KEY`. Defaults live in [`crate::defaults::mureka`].

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenAssetRole, GenHandle, GenRequest,
    GenResult, GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::warn_unconsumed_params;
use serde_json::{json, Value};

use crate::defaults::mureka;

/// Mureka music gen-task configuration.
#[derive(Debug, Clone)]
pub struct MurekaMusicConfig {
    pub model: String,
    pub api_key: String,
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Mureka music HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Mureka music HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
fn parse_err(e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse Mureka music response: {e}"),
    )
}

/// Keys forwarded from [`GenRequest::params`] into the submit body. `model`,
/// `prompt`, and `is_instrumental` are excluded: `model`/`prompt` are set
/// explicitly, and `is_instrumental` only selects the endpoint (and suppresses
/// `lyrics`). The typed [`MusicParams`](orchest_protocol::MusicParams)
/// counterparts (`lyrics`, `instrumental`) take precedence over the raw
/// `lyrics`/`is_instrumental` keys; the remaining keys have no typed form.
const PASSTHROUGH_KEYS: &[&str] = &[
    "lyrics",
    "n",
    "reference_id",
    "vocal_id",
    "melody_id",
    "gender",
];

/// Whether this request asks for instrumental generation: typed
/// `music.instrumental` first, raw `params.is_instrumental` as the
/// backward-compat fallback. Shared by [`build_submit_body`] (lyrics
/// suppression) and `submit` (endpoint selection) so the two never disagree.
fn is_instrumental(request: &GenRequest) -> bool {
    request
        .music
        .as_ref()
        .and_then(|m| m.instrumental)
        .or_else(|| {
            request
                .params
                .get("is_instrumental")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// Build the `/v1/song/generate` (or `/v1/instrumental/generate`) body: `model`
/// and `prompt`, plus passthrough params (`lyrics`, `n`, `reference_id`,
/// `vocal_id`, `melody_id`, `gender`) drawn from [`GenRequest::params`], with
/// typed [`MusicParams`](orchest_protocol::MusicParams) `lyrics` taking
/// precedence over the raw key. When the request is instrumental (typed
/// `music.instrumental` or raw `params.is_instrumental`), `lyrics` is omitted
/// (instrumental generation takes no lyrics). Any params key outside the
/// consumed set warns via [`warn_unconsumed_params`].
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    warn_unconsumed_params(
        "mureka",
        &[PASSTHROUGH_KEYS, &["is_instrumental"]].concat(),
        &request.params,
    );
    let is_instrumental = is_instrumental(request);
    let mut body = json!({ "model": model, "prompt": request.prompt });
    if let Some(params) = request.params.as_object() {
        for &key in PASSTHROUGH_KEYS {
            if key == "lyrics" && is_instrumental {
                continue;
            }
            if let Some(value) = params.get(key) {
                body[key] = value.clone();
            }
        }
    }
    if !is_instrumental {
        if let Some(lyrics) = request.music.as_ref().and_then(|m| m.lyrics.as_ref()) {
            body["lyrics"] = json!(lyrics);
        }
    }
    body
}

/// In-flight job state for the async submit -> poll -> fetch lifecycle.
enum JobState {
    /// Submitted; `instrumental` records which query endpoint to poll, and
    /// `trace_id` is carried into the result metadata.
    Pending {
        instrumental: bool,
        trace_id: String,
    },
    /// Completed; the result is returned (once) by `fetch`.
    Done(GenResult),
}

/// The Mureka music gen provider as the spine [`GenTask`].
pub struct MurekaMusicGen {
    config: MurekaMusicConfig,
    jobs: Mutex<HashMap<String, JobState>>,
}

impl MurekaMusicGen {
    pub fn new(config: MurekaMusicConfig) -> Self {
        Self {
            config,
            jobs: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, JobState>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The static descriptor the registry filters on for the mureka music dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("mureka", mureka::DEFAULT_MODEL, Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Audio])
}

/// Build a [`MurekaMusicGen`] from a registry [`ProviderConfig`]. The API key
/// is read from `cfg.api_key` or the `MUREKA_API_KEY` env var; the base URL
/// from `cfg.api_url` or `MUREKA_API_URL` or the default; `model` from
/// `cfg.model` or the default `auto`. A trailing `/` on the base URL is
/// trimmed.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<MurekaMusicGen, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .or_else(|| {
            std::env::var(mureka::API_KEY_ENV)
                .ok()
                .filter(|s| !s.is_empty())
        })
        .ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::MissingApiKey,
                "mureka music requires api_key (set MUREKA_API_KEY)",
            )
        })?;
    let model = if cfg.model.is_empty() {
        mureka::DEFAULT_MODEL.to_string()
    } else {
        cfg.model.clone()
    };
    let api_base_url = cfg
        .api_url
        .clone()
        .or_else(|| {
            std::env::var(mureka::API_URL_ENV)
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| mureka::API_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(MurekaMusicGen::new(MurekaMusicConfig {
        model,
        api_key,
        api_base_url,
    }))
}

/// Project a completed `/v1/song/query` (or instrumental) response onto the
/// spine result. `status: "succeeded"` with `choices[].url` yields audio URL
/// assets; `choices[].id` (the reusable song id) is carried in
/// `diagnostic_metadata.song_ids`. `status: "failed"` is a provider task
/// failure.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
pub fn build_result(response: &Value, trace_id: &str) -> Result<GenResult, ProtocolError> {
    let status = response.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "failed" {
        return Err(build_result_failed(response));
    }
    let choices = response.get("choices").and_then(Value::as_array);
    let assets = choices
        .into_iter()
        .flatten()
        .filter_map(|choice| choice.get("url").and_then(Value::as_str))
        .map(|url| GenAsset::Url {
            url: url.to_string(),
            media_type: Some("audio/mpeg".to_string()),
            role: GenAssetRole::Primary,
        })
        .collect::<Vec<_>>();
    let song_ids = choices
        .into_iter()
        .flatten()
        .filter_map(|choice| choice.get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let diagnostic_metadata = json!({
        "provider": "mureka",
        "trace_id": trace_id,
        "song_ids": song_ids,
    });
    Ok(GenResult {
        assets,
        diagnostic_metadata,
        timed_text: None,
        duration_secs: None,
    })
}
/// Extract a [`ErrorCode::ProviderTaskFailed`] from a `status: "failed"`
/// response. The `error` field (if present and non-empty) is the message;
/// otherwise a generic message is used.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace convention)
fn build_result_failed(response: &Value) -> ProtocolError {
    let message = response
        .get("error")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "Mureka music task failed".to_string());
    ProtocolError::new(ErrorCode::ProviderTaskFailed, message)
}

#[async_trait]
impl GenTask for MurekaMusicGen {
    fn provider_name(&self) -> &str {
        "mureka"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("mureka", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Audio])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let is_instrumental = is_instrumental(&req);
        let endpoint = if is_instrumental {
            "/v1/instrumental/generate"
        } else {
            "/v1/song/generate"
        };
        let body = build_submit_body(&self.config.model, &req);
        let response = crate::http::shared_client()
            .post(format!("{}{endpoint}", self.config.api_base_url))
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
        let value: Value = serde_json::from_str(&text).map_err(parse_err)?;
        let task_id = value
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::ProviderHttpError,
                    "Mureka music submit response missing id",
                )
            })?
            .to_string();
        let trace_id = value
            .get("trace_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.lock().insert(
            task_id.clone(),
            JobState::Pending {
                instrumental: is_instrumental,
                trace_id,
            },
        );
        Ok(GenHandle {
            id: task_id,
            provider: Some("mureka".to_string()),
        })
    }

    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
        let (instrumental, trace_id) = {
            let mut jobs = self.lock();
            match jobs.get_mut(&handle.id) {
                Some(JobState::Done(_)) => return Ok(GenStatus::Done),
                Some(JobState::Pending {
                    instrumental,
                    trace_id,
                }) => (*instrumental, trace_id.clone()),
                None => {
                    return Err(ProtocolError::new(
                        ErrorCode::InvalidRequest,
                        "unknown mureka job handle",
                    ))
                }
            }
        };
        let endpoint = if instrumental {
            "/v1/instrumental/query/"
        } else {
            "/v1/song/query/"
        };
        let response = crate::http::shared_client()
            .get(format!(
                "{}{endpoint}{}",
                self.config.api_base_url, handle.id
            ))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        let value: Value = serde_json::from_str(&text).map_err(parse_err)?;
        let job_status = value.get("status").and_then(Value::as_str).unwrap_or("");
        let gen_status = match job_status {
            "preparing" => GenStatus::Pending,
            "running" => GenStatus::Running,
            "succeeded" => {
                let result = build_result(&value, &trace_id)?;
                let mut jobs = self.lock();
                jobs.insert(handle.id.clone(), JobState::Done(result));
                GenStatus::Done
            }
            "failed" => return Err(build_result_failed(&value)),
            other => {
                return Err(ProtocolError::new(
                    ErrorCode::ProviderHttpError,
                    format!("Mureka music unknown status: {other}"),
                ))
            }
        };
        Ok(gen_status)
    }

    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        let removed = self.lock().remove(&handle.id);
        match removed {
            Some(JobState::Done(result)) => Ok(result),
            Some(JobState::Pending { .. }) => Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "mureka job not done, poll first",
            )),
            None => Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "unknown mureka job handle (already fetched or never submitted)",
            )),
        }
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
    fn submit_body_typed_lyrics_wins_over_raw() {
        let body = build_submit_body(
            "auto",
            &request_with_music(
                "r&b, slow, male vocal",
                json!({"lyrics": "raw lyrics"}),
                MusicParams {
                    lyrics: Some("typed lyrics".to_string()),
                    ..MusicParams::default()
                },
            ),
        );
        assert_eq!(body["lyrics"], "typed lyrics");
    }

    #[test]
    fn submit_body_typed_instrumental_omits_lyrics() {
        let body = build_submit_body(
            "auto",
            &request_with_music(
                "orchestral",
                json!({"lyrics": "should be dropped"}),
                MusicParams {
                    lyrics: Some("typed lyrics".to_string()),
                    instrumental: Some(true),
                    ..MusicParams::default()
                },
            ),
        );
        assert!(body.get("lyrics").is_none());
    }

    #[test]
    fn is_instrumental_reads_typed_first() {
        let req = request_with_music(
            "p",
            json!({"is_instrumental": false}),
            MusicParams {
                instrumental: Some(true),
                ..MusicParams::default()
            },
        );
        assert!(is_instrumental(&req));

        // Raw key remains the fallback when the typed field is unset.
        let req = request("p", json!({"is_instrumental": true}));
        assert!(is_instrumental(&req));
        let req = request("p", json!({}));
        assert!(!is_instrumental(&req));
    }

    #[test]
    fn submit_body_sets_model_prompt_and_passes_through_lyrics() {
        let body = build_submit_body(
            "auto",
            &request(
                "r&b, slow, male vocal",
                json!({"lyrics": "la la la", "n": 2, "reference_id": "ref_1"}),
            ),
        );
        assert_eq!(body["model"], "auto");
        assert_eq!(body["prompt"], "r&b, slow, male vocal");
        assert_eq!(body["lyrics"], "la la la");
        assert_eq!(body["n"], 2);
        assert_eq!(body["reference_id"], "ref_1");
        // excluded keys are not present
        assert!(body.get("is_instrumental").is_none());
        assert!(body.get("model_override").is_none());
    }

    #[test]
    fn submit_body_instrumental_omits_lyrics() {
        let body = build_submit_body(
            "auto",
            &request(
                "orchestral",
                json!({"is_instrumental": true, "lyrics": "should be dropped", "n": 1}),
            ),
        );
        assert_eq!(body["model"], "auto");
        assert_eq!(body["prompt"], "orchestral");
        assert_eq!(body["n"], 1);
        assert!(body.get("lyrics").is_none());
        assert!(body.get("is_instrumental").is_none());
    }

    #[test]
    fn submit_body_skips_model_and_prompt_keys_in_params() {
        let body = build_submit_body(
            "auto",
            &request(
                "real prompt",
                json!({"model": "mureka-9", "prompt": "injected"}),
            ),
        );
        assert_eq!(body["model"], "auto");
        assert_eq!(body["prompt"], "real prompt");
    }

    #[test]
    fn build_result_succeeded_yields_audio_assets_and_song_ids() {
        let result = build_result(
            &json!({
                "status": "succeeded",
                "choices": [
                    {"url": "https://mureka/track1.mp3", "id": "song_aaa"},
                    {"url": "https://mureka/track2.mp3", "id": "song_bbb"}
                ]
            }),
            "trace-xyz",
        )
        .unwrap();
        assert_eq!(
            result.assets,
            vec![
                GenAsset::Url {
                    url: "https://mureka/track1.mp3".to_string(),
                    media_type: Some("audio/mpeg".to_string()),
                    role: GenAssetRole::Primary,
                },
                GenAsset::Url {
                    url: "https://mureka/track2.mp3".to_string(),
                    media_type: Some("audio/mpeg".to_string()),
                    role: GenAssetRole::Primary,
                },
            ]
        );
        assert_eq!(result.diagnostic_metadata["provider"], "mureka");
        assert_eq!(result.diagnostic_metadata["trace_id"], "trace-xyz");
        assert_eq!(result.diagnostic_metadata["song_ids"][0], "song_aaa");
        assert_eq!(result.diagnostic_metadata["song_ids"][1], "song_bbb");
    }

    #[test]
    fn build_result_failed_is_error() {
        let err = build_result(
            &json!({"status": "failed", "error": "content policy violation"}),
            "trace-1",
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ProviderTaskFailed);
        assert!(err.message.contains("content policy violation"));
    }

    #[test]
    fn build_result_failed_without_error_field() {
        let err = build_result(&json!({"status": "failed"}), "trace-2").unwrap_err();
        assert_eq!(err.code, ErrorCode::ProviderTaskFailed);
        assert!(err.message.contains("Mureka music task failed"));
    }

    #[test]
    fn build_result_succeeded_with_no_choices_yields_empty_assets() {
        let result = build_result(&json!({"status": "succeeded", "choices": []}), "t").unwrap();
        assert!(result.assets.is_empty());
        assert_eq!(
            result.diagnostic_metadata["song_ids"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
