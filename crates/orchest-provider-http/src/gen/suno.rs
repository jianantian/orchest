//! Suno music generation as the spine [`GenTask`] (Issue 007). Suno has no
//! official public API; this dialect targets a third-party proxy
//! (`https://api.sunoapi.org`) whose base URL is configurable via
//! `SUNO_API_URL`.
//!
//! The proxy is **asynchronous**: `POST /api/v1/generate` returns a `taskId`
//! immediately, and `GET /api/v1/generate/record-info?taskId=` is polled until
//! `status == "SUCCESS"` yields a `response` array of (typically two) tracks.
//! Each completed generation is cached in an in-process `Mutex<HashMap>` so the
//! spine submit -> poll -> fetch lifecycle holds without re-generating on
//! `fetch`.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenHandle, GenRequest, GenResult,
    GenStatus, GenTask, Modality, ProtocolError, TimedSegment, TimedText,
};
use orchest_provider_core::registry::ProviderConfig;
use serde_json::{json, Value};

/// Suno music gen-task configuration.
#[derive(Debug, Clone)]
pub struct SunoMusicConfig {
    pub model: String,
    pub api_key: String,
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Suno music HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Suno music HTTP {code}: {body}"),
    )
    .with_status(code)
}

/// Keys from [`GenRequest::params`] forwarded verbatim onto the Suno submit
/// body. `model`/`prompt`/`lyrics`/`instrumental` are handled explicitly (they
/// map onto Suno's `customMode`/`prompt`/`instrumental` fields) and so are
/// excluded from passthrough.
const PASSTHROUGH_PARAMS: &[&str] = &[
    "style",
    "title",
    "negativeTags",
    "vocalGender",
    "styleWeight",
    "weirdnessConstraint",
    "audioWeight",
    "personaId",
    "personaModel",
];

/// Build the `/api/v1/generate` body. Defaults to non-custom mode
/// (`customMode: false`, `instrumental: false`) with `prompt` as the creative
/// description. When `params.lyrics` is present the dialect switches to custom
/// mode: `customMode` becomes `true` and `prompt` is replaced by the lyrics
/// (Suno's custom-mode prompt field carries the lyrics). `params.instrumental`
/// toggles instrumental-only generation. The remaining Suno knobs (`style`,
/// `title`, `negativeTags`, `vocalGender`, `styleWeight`, `weirdnessConstraint`,
/// `audioWeight`, `personaId`, `personaModel`) pass through when present.
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    let mut body = json!({
        "model": model,
        "customMode": false,
        "instrumental": false,
        "callBackUrl": "https://localhost/suno-callback",
        "prompt": request.prompt,
    });

    if let Some(params) = request.params.as_object() {
        // Custom mode: lyrics present -> customMode=true, prompt becomes the
        // lyrics (Suno's custom-mode prompt field carries the lyrics).
        if let Some(lyrics) = params.get("lyrics").and_then(Value::as_str) {
            if !lyrics.is_empty() {
                body["customMode"] = json!(true);
                body["prompt"] = json!(lyrics);
            }
        }

        if let Some(instrumental) = params.get("instrumental") {
            if instrumental.as_bool().unwrap_or(false) {
                body["instrumental"] = json!(true);
            }
        }

        // Override callBackUrl from params if provided, so callers can set a
        // real webhook endpoint. Keep the default dummy URL for polling mode.
        if let Some(cb) = params.get("callBackUrl").and_then(Value::as_str) {
            if !cb.is_empty() {
                body["callBackUrl"] = json!(cb);
            }
        }

        for key in PASSTHROUGH_PARAMS {
            if let Some(value) = params.get(*key) {
                body[*key] = value.clone();
            }
        }
    }

    body
}

/// Project a completed `record-info` `data` payload onto the spine result.
/// `data.response` is an array of track objects; each item's `audioUrl` becomes
/// a [`GenAsset::Url`] (Suno serves `.mp3`). The track titles are surfaced in
/// `diagnostic_metadata` for traceability.
fn build_result(data: &Value) -> GenResult {
    let tracks = data
        .get("response")
        .and_then(|r| r.get("sunoData"))
        .and_then(Value::as_array);

    let assets = tracks
        .map(|tracks| {
            tracks
                .iter()
                .filter_map(|track| {
                    track
                        .get("audioUrl")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(|url| GenAsset::Url {
                            url: url.to_string(),
                            media_type: Some("audio/mpeg".to_string()),
                        })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let titles: Vec<String> = tracks
        .map(|tracks| {
            tracks
                .iter()
                .map(|t| t.get("title").and_then(Value::as_str).unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();

    let cover_url = tracks
        .and_then(|t| t.first())
        .and_then(|t| t.get("imageUrl").or_else(|| t.get("image_url")))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());

    let duration: Option<f64> = tracks
        .and_then(|t| t.first())
        .and_then(|t| t.get("duration").and_then(Value::as_f64));

    let diagnostic_metadata = json!({
        "provider": "suno",
        "titles": titles,
        "cover_url": cover_url,
        "duration_secs": duration,
    });

    GenResult {
        assets,
        diagnostic_metadata,
        timed_text: None,
    }
}

/// The primary track's id (Suno's `audioId`) from a `record-info` `data` payload.
/// Needed to request that track's aligned lyrics.
fn primary_audio_id(data: &Value) -> Option<String> {
    data.get("response")
        .and_then(|r| r.get("sunoData"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|t| t.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// Map a `get-timestamped-lyrics` `data` payload onto [`TimedText`]. Text is
/// kept provider-verbatim (Suno's `word` chunks may carry section tags/newlines);
/// the consumer cleans it at render time. Returns `None` when there are no words.
fn parse_timed_text(data: &Value) -> Option<TimedText> {
    let segments: Vec<TimedSegment> = data
        .get("alignedWords")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|w| {
            let text = w.get("word").and_then(Value::as_str)?.to_string();
            let start = w.get("startS").and_then(Value::as_f64)?;
            let end = w.get("endS").and_then(Value::as_f64);
            Some(TimedSegment { text, start, end })
        })
        .collect();
    (!segments.is_empty()).then_some(TimedText { segments })
}

/// In-flight job state for the async submit -> poll -> fetch lifecycle.
enum JobState {
    Pending,
    Done(GenResult),
}


// TODO: callback URL support. The Suno API supports webhook callbacks
// (text / first / complete stages) via the `callBackUrl` field. Currently
// the adapter uses a dummy URL and relies on polling. To support callbacks:
// 1. Accept an optional `callBackUrl` in `SunoMusicConfig` (or GenRequest
//    params, which already works as a passthrough).
// 2. Expose a way for the caller to register a webhook endpoint.
// 3. Either store the callback data in a shared cache (like the current
//    `JobState`) or notify via a channel.
// This is low priority — polling works fine for 30-60s generation times.
/// The Suno music gen provider as the spine [`GenTask`].
pub struct SunoMusicGen {
    config: SunoMusicConfig,
    jobs: Mutex<HashMap<String, JobState>>,
}

impl SunoMusicGen {
    pub fn new(config: SunoMusicConfig) -> Self {
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

    /// Fetch forced-aligned lyrics for one track. Best-effort: any failure
    /// (network, non-200, no words) yields `(None, None)` so it never breaks
    /// the poll. Returns the aligned text plus the provider's overall
    /// confidence (`hootCer`) for the consumer's trust decision.
    async fn fetch_timed_text(
        &self,
        task_id: &str,
        audio_id: &str,
    ) -> (Option<TimedText>, Option<f64>) {
        let resp = crate::http::shared_client()
            .post(format!(
                "{}/api/v1/generate/get-timestamped-lyrics",
                self.config.api_base_url
            ))
            .bearer_auth(&self.config.api_key)
            .json(&json!({ "taskId": task_id, "audioId": audio_id }))
            .send()
            .await;
        let Ok(resp) = resp else { return (None, None) };
        let Ok(value) = resp.json::<Value>().await else { return (None, None) };
        if value.get("code").and_then(Value::as_i64) != Some(200) {
            return (None, None);
        }
        let data = value.get("data");
        let hoot_cer = data.and_then(|d| d.get("hootCer")).and_then(Value::as_f64);
        let timed_text = data.and_then(parse_timed_text);
        (timed_text, hoot_cer)
    }
}

/// The static descriptor the registry filters on for the Suno music dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        "suno",
        crate::defaults::suno::DEFAULT_MODEL,
        Capability::GenTask,
    )
    .with_input_modalities([Modality::Text])
    .with_output_modalities([Modality::Audio])
}

/// Build a [`SunoMusicGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<SunoMusicGen, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .or_else(|| {
            std::env::var(crate::defaults::suno::API_KEY_ENV)
                .ok()
                .filter(|s| !s.is_empty())
        })
        .ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::MissingApiKey,
                "suno music requires api_key (set SUNO_API_KEY)",
            )
        })?;
    let model = if cfg.model.is_empty() {
        crate::defaults::suno::DEFAULT_MODEL.to_string()
    } else {
        cfg.model.clone()
    };
    let api_base_url = cfg
        .api_url
        .clone()
        .or_else(|| {
            std::env::var(crate::defaults::suno::API_URL_ENV)
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| crate::defaults::suno::API_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(SunoMusicGen::new(SunoMusicConfig {
        model,
        api_key,
        api_base_url,
    }))
}
#[async_trait]
impl GenTask for SunoMusicGen {
    fn provider_name(&self) -> &str {
        "suno"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("suno", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Audio])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = crate::http::shared_client()
            .post(format!("{}/api/v1/generate", self.config.api_base_url))
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
                format!("failed to parse Suno music response: {e}"),
            )
        })?;
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
        if code != 200 {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Suno music submit failed (code={code})"));
            return Err(ProtocolError::new(ErrorCode::ProviderTaskFailed, message));
        }
        let task_id = value
            .pointer("/data/taskId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::ProviderHttpError,
                    "Suno music submit response missing data.taskId",
                )
            })?
            .to_string();
        self.lock().insert(task_id.clone(), JobState::Pending);
        Ok(GenHandle {
            id: task_id,
            provider: Some("suno".to_string()),
        })
    }

    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
        let response = crate::http::shared_client()
            .get(format!(
                "{}/api/v1/generate/record-info?taskId={}",
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
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            ProtocolError::new(
                ErrorCode::ProviderHttpError,
                format!("failed to parse Suno music status response: {e}"),
            )
        })?;
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
        if code != 200 {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Suno music poll failed (code={code})"));
            return Err(ProtocolError::new(ErrorCode::ProviderTaskFailed, message));
        }
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let job_status = data
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match job_status {
            "PENDING" => {
                let mut jobs = self.lock();
                jobs.entry(handle.id.clone()).or_insert(JobState::Pending);
                Ok(GenStatus::Pending)
            }
            "SEARCHING" | "FIRST" => {
                let mut jobs = self.lock();
                jobs.entry(handle.id.clone()).or_insert(JobState::Pending);
                Ok(GenStatus::Running)
            }
            "SUCCESS" => {
                let mut result = build_result(&data);
                // Secondary fetch: forced-aligned lyrics for the primary track.
                // A separate Suno endpoint (taskId + audioId); its result fills
                // GenResult.timed_text without touching the submit→poll→fetch
                // trait shape. Best-effort — a failure leaves timed_text = None
                // and the consumer falls back to its own estimate.
                if let Some(audio_id) = primary_audio_id(&data) {
                    let (timed_text, hoot_cer) =
                        self.fetch_timed_text(&handle.id, &audio_id).await;
                    result.timed_text = timed_text;
                    if let (Some(cer), Some(obj)) =
                        (hoot_cer, result.diagnostic_metadata.as_object_mut())
                    {
                        obj.insert("alignment_hoot_cer".to_string(), json!(cer));
                    }
                }
                self.lock()
                    .insert(handle.id.clone(), JobState::Done(result));
                Ok(GenStatus::Done)
            }
            "FAILED" => Err(ProtocolError::new(
                ErrorCode::ProviderTaskFailed,
                format!("Suno music task {} failed", handle.id),
            )),
            other => Err(ProtocolError::new(
                ErrorCode::ProviderTaskFailed,
                format!("Suno music task {} unknown status: {other}", handle.id),
            )),
        }
    }

    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        let state = self.lock().remove(&handle.id);
        match state {
            Some(JobState::Done(result)) => Ok(result),
            Some(JobState::Pending) => Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "job not done, poll first",
            )),
            None => Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "unknown gen handle (already fetched or never submitted)",
            )),
        }
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
    fn parse_timed_text_maps_aligned_words_verbatim() {
        // Shape returned by get-timestamped-lyrics (observed live). Word chunks
        // carry section tags/newlines; kept verbatim for the consumer to clean.
        let data = json!({
            "alignedWords": [
                {"word": "[Verse 1]\n晨光爬上窗台\n", "startS": 11.011, "endS": 16.676, "success": true},
                {"word": "你还在睡\n\n", "startS": 16.835, "endS": 21.638, "success": true}
            ],
            "hootCer": 0.6
        });
        let tt = parse_timed_text(&data).unwrap();
        assert_eq!(tt.segments.len(), 2);
        assert_eq!(tt.segments[0].start, 11.011);
        assert_eq!(tt.segments[0].end, Some(16.676));
        assert!(tt.segments[0].text.contains("晨光爬上窗台"));
        assert!(tt.segments[0].text.contains("[Verse 1]")); // verbatim, not cleaned
    }

    #[test]
    fn parse_timed_text_none_when_no_words() {
        assert!(parse_timed_text(&json!({ "alignedWords": [] })).is_none());
        assert!(parse_timed_text(&json!({})).is_none());
    }

    #[test]
    fn primary_audio_id_takes_first_track() {
        let data = json!({ "response": { "sunoData": [{"id": "aud-1"}, {"id": "aud-2"}] } });
        assert_eq!(primary_audio_id(&data), Some("aud-1".to_string()));
        assert_eq!(primary_audio_id(&json!({})), None);
    }

    #[test]
    fn submit_body_non_custom_mode_uses_prompt_as_description() {
        let body = build_submit_body("V5_5", &request("a calm piano track", json!({})));
        assert_eq!(body["model"], "V5_5");
        assert_eq!(body["customMode"], false);
        assert_eq!(body["instrumental"], false);
        assert_eq!(body["callBackUrl"], "https://localhost/suno-callback");
        assert_eq!(body["prompt"], "a calm piano track");
    }

    #[test]
    fn submit_body_custom_mode_when_lyrics_present() {
        let body = build_submit_body(
            "V5_5",
            &request(
                "ignored description",
                json!({
                    "lyrics": "la la la",
                    "style": "lofi",
                    "title": "my song",
                }),
            ),
        );
        assert_eq!(body["customMode"], true);
        // In custom mode the prompt carries the lyrics.
        assert_eq!(body["prompt"], "la la la");
        assert_eq!(body["style"], "lofi");
        assert_eq!(body["title"], "my song");
    }

    #[test]
    fn submit_body_instrumental_toggle_and_passthrough() {
        let body = build_submit_body(
            "V5",
            &request(
                "ambient drone",
                json!({
                    "instrumental": true,
                    "negativeTags": "Heavy Metal",
                    "vocalGender": "f",
                    "styleWeight": 0.5,
                    "weirdnessConstraint": 0.2,
                    "audioWeight": 0.8,
                    "personaId": "p-1",
                    "personaModel": "style_persona",
                }),
            ),
        );
        assert_eq!(body["instrumental"], true);
        assert_eq!(body["negativeTags"], "Heavy Metal");
        assert_eq!(body["vocalGender"], "f");
        assert_eq!(body["styleWeight"], 0.5);
        assert_eq!(body["weirdnessConstraint"], 0.2);
        assert_eq!(body["audioWeight"], 0.8);
        assert_eq!(body["personaId"], "p-1");
        assert_eq!(body["personaModel"], "style_persona");
        // Non-custom mode keeps the original prompt.
        assert_eq!(body["prompt"], "ambient drone");
        assert_eq!(body["customMode"], false);
    }

    #[test]
    fn submit_body_ignores_empty_lyrics() {
        let body = build_submit_body(
            "V5_5",
            &request("a calm piano track", json!({ "lyrics": "" })),
        );
        assert_eq!(body["customMode"], false);
        assert_eq!(body["prompt"], "a calm piano track");
    }

    #[test]
    fn submit_body_ignores_non_string_lyrics() {
        // Regression: null/non-string lyrics must NOT trigger custom mode or
        // overwrite the prompt. Previously `params.get("lyrics")` matched
        // Value::Null and clobbered prompt with null.
        for lyrics in [Value::Null, json!(123), json!(["a"])] {
            let body = build_submit_body(
                "V5_5",
                &request("a calm piano track", json!({ "lyrics": lyrics })),
            );
            assert_eq!(
                body["customMode"], false,
                "lyrics={lyrics} should not trigger custom mode"
            );
            assert_eq!(
                body["prompt"], "a calm piano track",
                "lyrics={lyrics} should not overwrite prompt"
            );
        }
    }

    #[test]
    fn submit_body_instrumental_false_keeps_default() {
        let body = build_submit_body(
            "V5_5",
            &request("a calm piano track", json!({ "instrumental": false })),
        );
        assert_eq!(body["instrumental"], false);
    }

    #[test]
    fn build_result_extracts_two_assets_and_titles() {
        let data = json!({
            "status": "SUCCESS",
            "response": {
                "sunoData": [
                    { "audioUrl": "https://suno/track1.mp3", "title": "First" },
                    { "audioUrl": "https://suno/track2.mp3", "title": "Second" },
                ]
            }
        });
        let result = build_result(&data);
        assert_eq!(
            result.assets,
            vec![
                GenAsset::Url {
                    url: "https://suno/track1.mp3".to_string(),
                    media_type: Some("audio/mpeg".to_string()),
                },
                GenAsset::Url {
                    url: "https://suno/track2.mp3".to_string(),
                    media_type: Some("audio/mpeg".to_string()),
                },
            ]
        );
        assert_eq!(result.diagnostic_metadata["provider"], "suno");
        assert_eq!(result.diagnostic_metadata["titles"][0], "First");
        assert_eq!(result.diagnostic_metadata["titles"][1], "Second");
    }

    #[test]
    fn build_result_skips_items_without_audio_url() {
        let data = json!({
            "status": "SUCCESS",
            "response": {
                "sunoData": [
                    { "audioUrl": "https://suno/track1.mp3", "title": "First" },
                    { "audioUrl": "", "title": "Empty" },
                ]
            }
        });
        let result = build_result(&data);
        assert_eq!(result.assets.len(), 1);
        assert_eq!(
            result.assets[0],
            GenAsset::Url {
                url: "https://suno/track1.mp3".to_string(),
                media_type: Some("audio/mpeg".to_string()),
            }
        );
    }

    #[test]
    fn build_result_empty_response_yields_no_assets() {
        let data = json!({ "status": "SUCCESS", "response": {"sunoData": []} });
        let result = build_result(&data);
        assert!(result.assets.is_empty());
        assert_eq!(result.diagnostic_metadata["provider"], "suno");
        assert_eq!(result.diagnostic_metadata["titles"], json!([]));
    }
}
