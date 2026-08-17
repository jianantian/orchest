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
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenAssetRole, GenHandle, GenRequest,
    GenResult, GenStatus, GenTask, Modality, ProtocolError, TimedSegment, TimedText, TrackMeta,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::warn_unconsumed_params;
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
/// excluded from passthrough. `personaId`/`personaModel` have no typed
/// [`MusicParams`](orchest_protocol::MusicParams) counterpart — raw `params`
/// remains their only entry point.
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

/// Raw-params keys the dialect consumes explicitly: `lyrics` switches to
/// custom mode, `instrumental` toggles vocal-less generation, `callBackUrl`
/// overrides the dummy polling callback. Together with [`PASSTHROUGH_PARAMS`]
/// this is the full consumed set for the unconsumed-key warning — every
/// typed-consumed key (`style`/`title`/`negativeTags`/…) already appears in
/// the passthrough whitelist under its wire name.
const EXPLICIT_PARAMS: &[&str] = &["lyrics", "instrumental", "callBackUrl"];

/// Per-request cap on the best-effort `get-timestamped-lyrics` call during
/// `fetch` — much tighter than the shared client's 300s default.
const TIMED_TEXT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Build the `/api/v1/generate` body. Defaults to non-custom mode
/// (`customMode: false`, `instrumental: false`) with `prompt` as the creative
/// description. When lyrics are present the dialect switches to custom mode:
/// `customMode` becomes `true` and `prompt` is replaced by the lyrics (Suno's
/// custom-mode prompt field carries the lyrics). `instrumental` toggles
/// instrumental-only generation.
///
/// Typed [`MusicParams`](orchest_protocol::MusicParams) fields take
/// precedence; raw `params` is the backward-compatible fallback for
/// `lyrics`/`instrumental` and the passthrough knobs (`style`, `title`,
/// `negativeTags`, `vocalGender`, `styleWeight`, `weirdnessConstraint`,
/// `audioWeight`), and remains the only entry for the dialect-specific extras
/// (`personaId`, `personaModel`, `callBackUrl`). Any other raw key warns via
/// [`warn_unconsumed_params`].
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    warn_unconsumed_params(
        "suno",
        &[PASSTHROUGH_PARAMS, EXPLICIT_PARAMS].concat(),
        &request.params,
    );

    let mut body = json!({
        "model": model,
        "customMode": false,
        "instrumental": false,
        "callBackUrl": "https://localhost/suno-callback",
        "prompt": request.prompt,
    });

    let typed = request.music.as_ref();
    let params = request.params.as_object();

    // Custom mode: lyrics present -> customMode=true, prompt becomes the
    // lyrics. Typed `music.lyrics` wins; raw `params.lyrics` is the
    // backward-compat fallback. Empty/non-string lyrics never trigger
    // custom mode.
    let lyrics = typed
        .and_then(|m| m.lyrics.as_deref())
        .or_else(|| params.and_then(|p| p.get("lyrics")).and_then(Value::as_str));
    if let Some(lyrics) = lyrics.filter(|l| !l.is_empty()) {
        body["customMode"] = json!(true);
        body["prompt"] = json!(lyrics);
    }

    // Instrumental toggle: typed first, raw fallback.
    let instrumental = typed.and_then(|m| m.instrumental).or_else(|| {
        params
            .and_then(|p| p.get("instrumental"))
            .and_then(Value::as_bool)
    });
    if instrumental == Some(true) {
        body["instrumental"] = json!(true);
    }

    if let Some(params) = params {
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

    // Typed knobs override the raw passthrough. Serializing MusicParams
    // yields only the set fields, already in Suno's camelCase wire spelling;
    // `lyrics`/`instrumental` went through the explicit handling above.
    if let Some(typed) = typed {
        if let Ok(Value::Object(knobs)) = serde_json::to_value(typed) {
            for (key, value) in knobs {
                if key != "lyrics" && key != "instrumental" {
                    body[key] = value;
                }
            }
        }
    }

    body
}

/// Project a completed `record-info` `data` payload onto the spine result.
/// `data.response` is an array of track objects; each item's `audioUrl` becomes
/// a [`GenAsset::Url`] with `role: Primary` (Suno serves `.mp3`). The first
/// track's `imageUrl` rides alongside as a `role: Cover` asset and its
/// `duration` lifts into the typed [`GenResult::duration_secs`] — both are
/// first-class product outputs, so they no longer ride in
/// `diagnostic_metadata`, which keeps only genuine diagnostics (the track
/// titles, for traceability). The cover and duration attach to the first
/// track, the same one `assets[0]` and [`TimedText`] point at; grouping
/// per-variant assets across the multiple tracks Suno returns is a separate
/// gap (Finding 1 note), not this change.
fn build_result(data: &Value) -> GenResult {
    let tracks = data
        .get("response")
        .and_then(|r| r.get("sunoData"))
        .and_then(Value::as_array);

    let mut assets = tracks
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
                            role: GenAssetRole::Primary,
                        })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let titles: Vec<String> = tracks
        .map(|tracks| {
            tracks
                .iter()
                .map(|t| {
                    t.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(cover_url) = tracks
        .and_then(|t| t.first())
        .and_then(|t| t.get("imageUrl").or_else(|| t.get("image_url")))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        assets.push(GenAsset::Url {
            url: cover_url.to_string(),
            // Suno serves cover art as .jpeg.
            media_type: Some("image/jpeg".to_string()),
            role: GenAssetRole::Cover,
        });
    }

    let duration_secs = tracks
        .and_then(|t| t.first())
        .and_then(|t| t.get("duration").and_then(Value::as_f64));

    let diagnostic_metadata = json!({
        "provider": "suno",
        "titles": titles,
    });

    GenResult {
        assets,
        diagnostic_metadata,
        timed_text: None,
        duration_secs,
        // Filled by `fetch` once the per-track aligned lyrics are in.
        track_meta: Vec::new(),
    }
}

/// The ids (Suno `audioId`) of every track with a non-empty `audioUrl` in a
/// `record-info` `data` payload, in order. Same predicate as the asset list,
/// so index `i` here is the same track the `i`-th Primary asset points at —
/// `fetch` requests each track's aligned lyrics by id.
fn audio_ids(data: &Value) -> Vec<String> {
    data.get("response")
        .and_then(|r| r.get("sunoData"))
        .and_then(Value::as_array)
        .map(|tracks| {
            tracks
                .iter()
                .filter(|track| {
                    track
                        .get("audioUrl")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty())
                })
                .filter_map(|t| t.get("id").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Per-track durations from a `record-info` `data` payload: one entry per
/// track with a non-empty `audioUrl` (the same predicate as the asset list,
/// so entries are index-aligned with the Primary assets). `None` when the
/// track reports no duration.
fn track_durations(data: &Value) -> Vec<Option<f64>> {
    data.get("response")
        .and_then(|r| r.get("sunoData"))
        .and_then(Value::as_array)
        .map(|tracks| {
            tracks
                .iter()
                .filter(|track| {
                    track
                        .get("audioUrl")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty())
                })
                .map(|t| t.get("duration").and_then(Value::as_f64))
                .collect()
        })
        .unwrap_or_default()
}

/// Pair per-track durations with the fetched per-track aligned lyrics into
/// one [`TrackMeta`] per Primary asset, index-aligned. Best-effort: a track
/// whose timed-lyrics call failed simply keeps `timed_text: None`.
fn assemble_track_meta(
    durations: Vec<Option<f64>>,
    timed_texts: Vec<Option<TimedText>>,
) -> Vec<TrackMeta> {
    durations
        .into_iter()
        .zip(timed_texts)
        .map(|(duration_secs, timed_text)| TrackMeta {
            duration_secs,
            timed_text,
        })
        .collect()
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
    Done {
        result: GenResult,
        /// The ids (Suno `audioId`) of every track with an audio URL, captured
        /// by `poll` from the `record-info` payload. `fetch` needs them to
        /// request each track's aligned lyrics; the payload itself is gone by
        /// then. Index-aligned with `durations` and the Primary assets.
        audio_ids: Vec<String>,
        /// Per-track durations, index-aligned with `audio_ids` and the
        /// Primary assets (`None` = track reported no duration).
        durations: Vec<Option<f64>>,
    },
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
    /// the fetch. Returns the aligned text plus the provider's overall
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
            // Best-effort side call: cap it tightly so a hanging endpoint
            // can't stall the main fetch behind the shared client's 300s
            // default.
            .timeout(TIMED_TEXT_TIMEOUT)
            .send()
            .await;
        let resp = match resp {
            Ok(resp) => resp,
            Err(e) => {
                tracing::debug!(task_id, error = %e, "suno timed-lyrics request failed");
                return (None, None);
            }
        };
        let value = match resp.json::<Value>().await {
            Ok(value) => value,
            Err(e) => {
                tracing::debug!(task_id, error = %e, "suno timed-lyrics response not JSON");
                return (None, None);
            }
        };
        let code = value.get("code").and_then(Value::as_i64);
        if code != Some(200) {
            tracing::debug!(
                task_id,
                code,
                "suno timed-lyrics endpoint returned non-200 code"
            );
            return (None, None);
        }
        let data = value.get("data");
        let hoot_cer = data.and_then(|d| d.get("hootCer")).and_then(Value::as_f64);
        let timed_text = data.and_then(parse_timed_text);
        if timed_text.is_none() {
            tracing::debug!(task_id, "suno timed-lyrics returned no aligned words");
        }
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
                // Store the generated result only. The secondary
                // get-timestamped-lyrics call belongs to `fetch` (Finding 1
                // layering) — `poll` may be repeated after SUCCESS, which
                // would re-fire the lyrics request every time.
                let result = build_result(&data);
                let audio_ids = audio_ids(&data);
                let durations = track_durations(&data);
                self.lock().insert(
                    handle.id.clone(),
                    JobState::Done {
                        result,
                        audio_ids,
                        durations,
                    },
                );
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
            Some(JobState::Done {
                mut result,
                audio_ids,
                durations,
            }) => {
                // Secondary calls: forced-aligned lyrics, one per track (Suno
                // returns two tracks per generation). A separate Suno endpoint
                // (taskId + audioId); its results fill GenResult.track_meta
                // without touching the submit→poll→fetch trait shape.
                // Best-effort — a failure leaves that track's timed_text =
                // None and the consumer falls back to its own estimate.
                // `fetch` is normally invoked once, so one extra read-only
                // request per track here is acceptable; they run sequentially
                // to keep the per-track order deterministic.
                let mut timed_texts = Vec::with_capacity(audio_ids.len());
                for (index, audio_id) in audio_ids.iter().enumerate() {
                    let (timed_text, hoot_cer) = self.fetch_timed_text(&handle.id, audio_id).await;
                    if index == 0 {
                        // Track 0 keeps filling the global fields, exactly as
                        // before multi-track support: the global timed_text /
                        // duration_secs mirror track_meta[0].
                        result.timed_text = timed_text.clone();
                        if let (Some(cer), Some(obj)) =
                            (hoot_cer, result.diagnostic_metadata.as_object_mut())
                        {
                            obj.insert("alignment_hoot_cer".to_string(), json!(cer));
                        }
                    }
                    timed_texts.push(timed_text);
                }
                result.track_meta = assemble_track_meta(durations, timed_texts);
                Ok(result)
            }
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
    use orchest_protocol::{MusicParams, VocalGender};

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
    fn parse_timed_text_missing_end_s_yields_none_end() {
        // Not all sources supply an end; `None` = unknown, the consumer
        // infers the span from the next segment's start if it needs one.
        let data = json!({ "alignedWords": [{"word": "hello", "startS": 1.25}] });
        let tt = parse_timed_text(&data).unwrap();
        assert_eq!(tt.segments[0].start, 1.25);
        assert_eq!(tt.segments[0].end, None);
    }

    #[test]
    fn audio_ids_match_every_track_with_audio_url() {
        // Same predicate as the asset list (every track with a non-empty
        // audioUrl), so index i here is the track the i-th Primary asset
        // points at — tracks without audio must not shift the alignment.
        let data = json!({ "response": { "sunoData": [
            {"id": "aud-no-url"},
            {"id": "aud-empty-url", "audioUrl": ""},
            {"id": "aud-2", "audioUrl": "https://suno/track2.mp3"},
            {"id": "aud-3", "audioUrl": "https://suno/track3.mp3"}
        ] } });
        assert_eq!(
            audio_ids(&data),
            vec!["aud-2".to_string(), "aud-3".to_string()]
        );
        assert_eq!(audio_ids(&json!({})), Vec::<String>::new());
        assert_eq!(
            audio_ids(&json!({ "response": { "sunoData": [{"id": "a"}] } })),
            Vec::<String>::new()
        );
    }

    #[test]
    fn track_meta_pairs_two_tracks_durations_with_timed_texts() {
        // Two-track payload: build_result lifts the first track's duration to
        // the global field; assemble_track_meta pairs every track's duration
        // with its fetched timed text, index-aligned with the Primary assets.
        let data = json!({ "response": { "sunoData": [
            {"id": "aud-1", "audioUrl": "https://suno/track1.mp3", "duration": 31.84},
            {"id": "aud-2", "audioUrl": "https://suno/track2.mp3", "duration": 28.5}
        ] } });
        let result = build_result(&data);
        assert_eq!(result.assets.len(), 2);
        assert_eq!(result.duration_secs, Some(31.84));

        let durations = track_durations(&data);
        assert_eq!(durations, vec![Some(31.84), Some(28.5)]);
        assert_eq!(
            audio_ids(&data),
            vec!["aud-1".to_string(), "aud-2".to_string()]
        );

        let tt = |text: &str| TimedText {
            segments: vec![TimedSegment {
                text: text.to_string(),
                start: 0.0,
                end: Some(1.0),
            }],
        };
        // Track 1's lyrics fetch failed (best-effort): only that entry is None.
        let track_meta = assemble_track_meta(durations, vec![Some(tt("first")), None]);
        assert_eq!(track_meta.len(), 2);
        assert_eq!(track_meta[0].duration_secs, Some(31.84));
        assert_eq!(track_meta[0].timed_text, Some(tt("first")));
        assert_eq!(track_meta[1].duration_secs, Some(28.5));
        assert_eq!(track_meta[1].timed_text, None);
        // Globals mirror track 0 (duration was lifted by build_result; the
        // global timed_text is set by `fetch` from the same track-0 value).
        assert_eq!(result.duration_secs, track_meta[0].duration_secs);
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
    fn submit_body_typed_music_fields_produce_full_knob_set() {
        // Every typed knob lands on the wire under Suno's camelCase spelling,
        // with no raw params involved at all.
        let body = build_submit_body(
            "V5_5",
            &request_with_music(
                "ignored description",
                json!({}),
                MusicParams {
                    lyrics: Some("typed lyrics".to_string()),
                    instrumental: Some(true),
                    style: Some("lofi".to_string()),
                    title: Some("typed title".to_string()),
                    negative_tags: Some("no choir".to_string()),
                    vocal_gender: Some(VocalGender::Female),
                    style_weight: Some(0.5),
                    weirdness_constraint: Some(0.2),
                    audio_weight: Some(0.8),
                },
            ),
        );
        assert_eq!(body["customMode"], true);
        assert_eq!(body["prompt"], "typed lyrics");
        assert_eq!(body["instrumental"], true);
        assert_eq!(body["style"], "lofi");
        assert_eq!(body["title"], "typed title");
        assert_eq!(body["negativeTags"], "no choir");
        assert_eq!(body["vocalGender"], "f");
        assert_eq!(body["styleWeight"], 0.5);
        assert_eq!(body["weirdnessConstraint"], 0.2);
        assert_eq!(body["audioWeight"], 0.8);
    }

    #[test]
    fn submit_body_typed_fields_win_over_raw_params() {
        let body = build_submit_body(
            "V5_5",
            &request_with_music(
                "a calm piano track",
                json!({
                    "lyrics": "raw lyrics",
                    "style": "raw style",
                    "negativeTags": "raw tags",
                    "vocalGender": "m",
                }),
                MusicParams {
                    lyrics: Some("typed lyrics".to_string()),
                    style: Some("typed style".to_string()),
                    negative_tags: Some("typed tags".to_string()),
                    vocal_gender: Some(VocalGender::Female),
                    ..MusicParams::default()
                },
            ),
        );
        assert_eq!(body["prompt"], "typed lyrics");
        assert_eq!(body["style"], "typed style");
        assert_eq!(body["negativeTags"], "typed tags");
        assert_eq!(body["vocalGender"], "f");
    }

    #[test]
    fn submit_body_typed_instrumental_wins_over_raw_params() {
        // Typed `instrumental: false` is an explicit choice, not "unset": it
        // beats a raw `instrumental: true`.
        let body = build_submit_body(
            "V5_5",
            &request_with_music(
                "a calm piano track",
                json!({ "instrumental": true }),
                MusicParams {
                    instrumental: Some(false),
                    ..MusicParams::default()
                },
            ),
        );
        assert_eq!(body["instrumental"], false);

        let body = build_submit_body(
            "V5_5",
            &request_with_music(
                "a calm piano track",
                json!({}),
                MusicParams {
                    instrumental: Some(true),
                    ..MusicParams::default()
                },
            ),
        );
        assert_eq!(body["instrumental"], true);
    }

    #[test]
    fn submit_body_typed_and_raw_params_combine() {
        // Raw params keeps the dialect-specific extras (personaId /
        // personaModel / callBackUrl) and the backward-compat knobs the typed
        // request leaves unset; typed fields fill in the rest.
        let body = build_submit_body(
            "V5_5",
            &request_with_music(
                "a calm piano track",
                json!({
                    "personaId": "p-1",
                    "personaModel": "style_persona",
                    "callBackUrl": "https://real.example/hook",
                    "styleWeight": 0.9,
                }),
                MusicParams {
                    style: Some("typed style".to_string()),
                    ..MusicParams::default()
                },
            ),
        );
        assert_eq!(body["personaId"], "p-1");
        assert_eq!(body["personaModel"], "style_persona");
        assert_eq!(body["callBackUrl"], "https://real.example/hook");
        assert_eq!(body["style"], "typed style");
        assert_eq!(body["styleWeight"], 0.9);
    }

    #[test]
    fn submit_body_raw_params_still_work_when_typed_absent() {
        // Backward compatibility: a caller that sets only raw params gets the
        // exact pre-typing body.
        let body = build_submit_body(
            "V5_5",
            &request(
                "a calm piano track",
                json!({ "lyrics": "raw lyrics", "vocalGender": "m" }),
            ),
        );
        assert_eq!(body["customMode"], true);
        assert_eq!(body["prompt"], "raw lyrics");
        assert_eq!(body["vocalGender"], "m");
    }

    // -------------------------------------------------------------------
    // unconsumed-key warning wiring
    // -------------------------------------------------------------------

    #[derive(Clone, Default)]
    struct SharedBuffer(Arc<StdMutex<Vec<u8>>>);

    impl std::io::Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl tracing_subscriber::fmt::MakeWriter<'_> for SharedBuffer {
        type Writer = SharedBuffer;

        fn make_writer(&self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn submit_body_warns_on_unconsumed_params_key() {
        let buffer = SharedBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buffer.clone())
            .with_ansi(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            build_submit_body(
                "V5_5",
                &request("a song", json!({ "genre": "indie folk", "style": "lofi" })),
            );
        });
        let logs = String::from_utf8(
            buffer
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        )
        .expect("tracing output is utf8");
        assert!(logs.contains("WARN"), "expected a WARN event: {logs}");
        assert!(
            logs.contains("genre"),
            "unknown key named in warning: {logs}"
        );
        assert!(
            !logs.contains("style"),
            "consumed key must not warn: {logs}"
        );
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
                    role: GenAssetRole::Primary,
                },
                GenAsset::Url {
                    url: "https://suno/track2.mp3".to_string(),
                    media_type: Some("audio/mpeg".to_string()),
                    role: GenAssetRole::Primary,
                },
            ]
        );
        assert_eq!(result.diagnostic_metadata["provider"], "suno");
        assert_eq!(result.diagnostic_metadata["titles"][0], "First");
        assert_eq!(result.diagnostic_metadata["titles"][1], "Second");
        // No cover/duration in the payload: no extra asset, no typed field.
        assert_eq!(result.duration_secs, None);
    }

    #[test]
    fn build_result_lifts_cover_and_duration_to_the_typed_surface() {
        // Finding 2: cover_url/duration_secs are first-class product outputs,
        // not diagnostics. The cover becomes a role-tagged asset appended
        // after the audio tracks, duration lands on the typed field, and
        // neither key remains in diagnostic_metadata.
        let data = json!({
            "status": "SUCCESS",
            "response": {
                "sunoData": [
                    {
                        "audioUrl": "https://suno/track1.mp3",
                        "title": "First",
                        "imageUrl": "https://suno/cover1.jpeg",
                        "duration": 31.84,
                    },
                    { "audioUrl": "https://suno/track2.mp3", "title": "Second" },
                ]
            }
        });
        let result = build_result(&data);
        assert_eq!(result.assets.len(), 3);
        assert_eq!(
            result.assets[2],
            GenAsset::Url {
                url: "https://suno/cover1.jpeg".to_string(),
                media_type: Some("image/jpeg".to_string()),
                role: GenAssetRole::Cover,
            }
        );
        assert_eq!(result.duration_secs, Some(31.84));
        assert!(result.diagnostic_metadata.get("cover_url").is_none());
        assert!(result.diagnostic_metadata.get("duration_secs").is_none());
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
                role: GenAssetRole::Primary,
            }
        );
    }

    #[test]
    fn build_result_empty_response_yields_no_assets() {
        let data = json!({ "status": "SUCCESS", "response": {"sunoData": []} });
        let result = build_result(&data);
        assert!(result.assets.is_empty());
        assert_eq!(result.duration_secs, None);
        assert_eq!(result.diagnostic_metadata["provider"], "suno");
        assert_eq!(result.diagnostic_metadata["titles"], json!([]));
    }

    // -------------------------------------------------------------------
    // fetch wiring: submit → poll → fetch against a local mock Suno server
    // -------------------------------------------------------------------

    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A minimal mock of the Suno proxy. Routes on the request path across the
    /// three endpoints the adapter calls, answering each from a canned body;
    /// `lyrics_status`/`lyrics_body` shape the get-timestamped-lyrics response
    /// so tests can exercise its failure path. Every request line
    /// (`METHOD path`) is recorded for assertions.
    struct MockSunoServer {
        base_url: String,
        request_lines: Arc<StdMutex<Vec<String>>>,
    }

    impl MockSunoServer {
        fn request_lines(&self) -> Vec<String> {
            self.request_lines
                .lock()
                .expect("request lines lock")
                .clone()
        }
    }

    async fn serve_suno(lyrics_status: u16, lyrics_body: &'static str) -> MockSunoServer {
        const SUBMIT_BODY: &str = r#"{"code":200,"msg":"success","data":{"taskId":"task-1"}}"#;
        const RECORD_INFO_BODY: &str = r#"{"code":200,"msg":"success","data":{"taskId":"task-1","status":"SUCCESS","response":{"sunoData":[{"id":"aud-1","audioUrl":"https://suno/track1.mp3","title":"First","imageUrl":"https://suno/cover1.jpeg","duration":31.84},{"id":"aud-2","audioUrl":"https://suno/track2.mp3","title":"Second"}]}}}"#;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener
            .local_addr()
            .expect("test server should have local address");
        let request_lines = Arc::new(StdMutex::new(Vec::new()));
        let captured = Arc::clone(&request_lines);

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let captured = Arc::clone(&captured);
                tokio::spawn(async move {
                    let request = read_request(&mut socket).await;
                    let request_line = request.lines().next().unwrap_or("").to_string();
                    captured
                        .lock()
                        .expect("request lines lock")
                        .push(request_line.clone());
                    let path = request_line.split_whitespace().nth(1).unwrap_or("");
                    let (status, body) =
                        if path.starts_with("/api/v1/generate/get-timestamped-lyrics") {
                            (lyrics_status, lyrics_body)
                        } else if path.starts_with("/api/v1/generate/record-info") {
                            (200, RECORD_INFO_BODY)
                        } else {
                            (200, SUBMIT_BODY)
                        };
                    let response = format!(
                        "HTTP/1.1 {status} Reply\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("test server should write response");
                });
            }
        });

        MockSunoServer {
            base_url: format!("http://{address}"),
            request_lines,
        }
    }

    /// Read one full HTTP request (headers + body, per content-length).
    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = socket.read(&mut chunk).await.expect("read request");
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
            let Some(header_end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&buffer[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if buffer.len() >= header_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8_lossy(&buffer).into_owned()
    }

    fn gen_for(server: &MockSunoServer) -> SunoMusicGen {
        SunoMusicGen::new(SunoMusicConfig {
            model: "V5".to_string(),
            api_key: "test-key".to_string(),
            api_base_url: server.base_url.clone(),
        })
    }

    #[tokio::test]
    async fn fetch_populates_timed_text_via_lyrics_endpoint() {
        const LYRICS_BODY: &str = r#"{"code":200,"msg":"success","data":{"alignedWords":[{"word":"[Verse 1]\n晨光爬上窗台\n","startS":11.011,"endS":16.676,"success":true},{"word":"你还在睡\n","startS":16.835,"endS":21.638,"success":true}],"hootCer":0.6}}"#;
        let server = serve_suno(200, LYRICS_BODY).await;
        let gen = gen_for(&server);

        let handle = gen
            .submit(request("a song", json!({})))
            .await
            .expect("submit");
        // Poll twice: a repeated poll after SUCCESS must not re-fire the
        // lyrics request (the bug this fixes).
        for _ in 0..2 {
            let status = gen.poll(&handle).await.expect("poll");
            assert_eq!(status, GenStatus::Done);
        }
        // The lyrics call belongs to fetch: poll alone must not fire it, no
        // matter how often SUCCESS is polled.
        assert!(
            !server
                .request_lines()
                .iter()
                .any(|l| l.contains("get-timestamped-lyrics")),
            "poll must not call the lyrics endpoint: {:?}",
            server.request_lines()
        );

        let result = gen.fetch(&handle).await.expect("fetch");
        let timed_text = result.timed_text.expect("timed_text populated");
        assert_eq!(timed_text.segments.len(), 2);
        assert_eq!(timed_text.segments[0].start, 11.011);
        assert_eq!(timed_text.segments[0].end, Some(16.676));
        assert!(timed_text.segments[0].text.contains("晨光爬上窗台"));
        assert_eq!(result.diagnostic_metadata["alignment_hoot_cer"], json!(0.6));
        // Two primary audio tracks plus the cover as a role-tagged asset;
        // duration rides the typed field, neither is in diagnostic_metadata.
        assert_eq!(result.assets.len(), 3);
        assert!(result.assets[..2].iter().all(|a| matches!(
            a,
            GenAsset::Url {
                role: GenAssetRole::Primary,
                ..
            }
        )));
        assert_eq!(
            result.assets.last(),
            Some(&GenAsset::Url {
                url: "https://suno/cover1.jpeg".to_string(),
                media_type: Some("image/jpeg".to_string()),
                role: GenAssetRole::Cover,
            })
        );
        assert_eq!(result.duration_secs, Some(31.84));
        assert!(result.diagnostic_metadata.get("cover_url").is_none());
        assert!(result.diagnostic_metadata.get("duration_secs").is_none());

        // One track_meta entry per Primary asset: durations from record-info
        // (track 2 reports none), timed text from the per-track lyrics call.
        // The global fields mirror track 0.
        assert_eq!(result.track_meta.len(), 2);
        assert_eq!(result.track_meta[0].duration_secs, Some(31.84));
        assert_eq!(result.track_meta[0].timed_text, Some(timed_text));
        assert_eq!(result.track_meta[1].duration_secs, None);
        assert_eq!(
            result.track_meta[1]
                .timed_text
                .as_ref()
                .map(|tt| tt.segments.len()),
            Some(2)
        );

        let lyrics_calls = server
            .request_lines()
            .iter()
            .filter(|l| l.contains("get-timestamped-lyrics"))
            .count();
        assert_eq!(
            lyrics_calls, 2,
            "lyrics endpoint called once per track, by fetch"
        );
    }

    #[tokio::test]
    async fn fetch_returns_result_when_lyrics_endpoint_fails() {
        // Non-JSON 500 from the lyrics endpoint: best-effort fallback — the
        // generation result still comes back intact with timed_text = None.
        let server = serve_suno(500, "upstream exploded").await;
        let gen = gen_for(&server);

        let handle = gen
            .submit(request("a song", json!({})))
            .await
            .expect("submit");
        let status = gen.poll(&handle).await.expect("poll");
        assert_eq!(status, GenStatus::Done);

        let result = gen.fetch(&handle).await.expect("fetch still succeeds");
        assert!(result.timed_text.is_none());
        assert!(result
            .diagnostic_metadata
            .get("alignment_hoot_cer")
            .is_none());
        assert_eq!(result.assets.len(), 3);
        assert_eq!(result.duration_secs, Some(31.84));
        // Best-effort per track: both entries keep their durations, both
        // timed texts are None.
        assert_eq!(result.track_meta.len(), 2);
        assert_eq!(result.track_meta[0].duration_secs, Some(31.84));
        assert_eq!(result.track_meta[0].timed_text, None);
        assert_eq!(result.track_meta[1].duration_secs, None);
        assert_eq!(result.track_meta[1].timed_text, None);
    }
}
