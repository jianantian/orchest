//! Music generation: the `/api/generate/*` pipeline over `GenTask`
//! (submit/poll/stream) plus LLM prompt enrichment.
//!
//! Extracted from the route handlers so routes stay thin — `generate` owns
//! all generation policy (validation, the empty-lyrics rule, enrichment,
//! submission); handlers only map results to HTTP.

use std::convert::Infallible;
use std::sync::Arc;

use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::ReceiverStream;

use orchest_protocol::{
    ChatModel, ContentBlock, GenAsset, GenHandle, GenRequest, GenStatus, GenTask, Message,
    RequestOptions, Role,
};

use crate::error::{AppError, AppResult};
use crate::gift::GiftStore;
use crate::prompts::MUSIC_PROMPT_SKILLS;

// ── Response types ──────────────────────────────────────────────────────────

/// Response for POST /api/generate/:id — submit.
#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub id: String,
    pub status: String,
    pub handle: Option<String>,
}

/// Response for GET /api/generate/:id/status — legacy poll.
#[derive(Debug, Serialize)]
pub struct GenStatusResponse {
    pub id: String,
    pub status: String,
    pub audio_url: Option<String>,
}

/// Structured prompt output from the LLM enrichment step.
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedPrompt {
    pub prompt: String,
    pub genre: Vec<String>,
    pub tempo: String,
    pub mood: Vec<String>,
    pub vocal_style: String,
    pub instrumentation: String,
    pub production: String,
    pub exclude: String,
    pub style_tags: Vec<String>,
}

impl EnrichedPrompt {
    pub fn fallback(style: &str) -> Self {
        Self {
            prompt: format!("{style}, high quality music production"),
            genre: vec![],
            tempo: String::new(),
            mood: vec![],
            vocal_style: String::new(),
            instrumentation: String::new(),
            production: String::new(),
            exclude: String::new(),
            style_tags: vec![],
        }
    }
}

impl std::fmt::Display for EnrichedPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.prompt)
    }
}

// ── Generate pipeline ───────────────────────────────────────────────────────

/// Typed generation inputs derived from the gift — exactly the fields
/// `GenRequest.params` is built from. `submit` takes these instead of
/// re-fetching the gift and re-deriving them from raw meta.
pub struct GenSubmission {
    pub lyrics: String,
    pub style: String,
    pub title: String,
    /// Vocal gender from the gift meta ("male"/"female").
    pub vocal: String,
    /// Gift kind — "instrumental" submits without lyrics or vocal gender.
    pub kind: String,
    /// Startup-resolved provider identity — selects the params dialect.
    pub provider: String,
}

/// The full generate pipeline: read the gift, validate lyrics, refuse to
/// generate a lyric-less song, enrich the style prompt via the prompt model
/// (falling back to the raw style on any enrichment failure — logged and
/// recorded as degraded), check the submitted style for artist names, and
/// submit the job.
///
/// All generation policy lives here; the route handler only maps the result
/// to HTTP. `provider` is the startup-resolved provider identity, not a
/// per-request env re-read.
pub async fn generate(
    gen_task: &Arc<dyn GenTask>,
    store: &GiftStore,
    prompt_model: &Arc<dyn ChatModel>,
    provider: &str,
    gift_id: &str,
) -> AppResult<GenerateResponse> {
    let gift = store.get(gift_id)?;

    // Idempotent submit: a gift already pending/running/done has a job in
    // flight or finished — return its current state instead of burning
    // provider quota on a duplicate submission. Only `failed` (or
    // never-submitted) gifts continue to a real submit below.
    if let Some(status @ ("pending" | "running" | "done")) = gift.gen_status.as_deref() {
        return Ok(GenerateResponse {
            id: gift_id.to_string(),
            status: status.to_string(),
            handle: gift.gen_handle.clone(),
        });
    }

    let meta = gift.meta();
    let lyrics = gift.lyrics.clone().unwrap_or_default();
    let style = meta.style_or_default();

    // Validate lyrics — warnings logged for Suno quality tuning.
    let validation = crate::tools::lyrics_validator::validate_lyrics(&lyrics);
    for w in &validation.warnings {
        tracing::warn!(gift_id, stage = "validate", warning = %w, "lyrics validation warning");
    }

    // A song gift with no lyrics is never what the user asked for: the provider
    // would invent its own (in its own language). Refuse rather than burn a
    // generation on it.
    if gift.kind != "instrumental" && lyrics.trim().is_empty() {
        return Err(AppError::BadRequest(
            "cannot generate a song with empty lyrics".to_string(),
        ));
    }

    let mut degraded: Vec<String> = Vec::new();
    let enriched = match generate_music_prompt(
        prompt_model.clone(),
        &MusicPromptInput {
            provider,
            lyrics: &lyrics,
            style,
            title: meta.title.as_deref().unwrap_or(""),
            vocal: meta.vocal_or_default(),
            scene: meta.scenario.as_deref().unwrap_or(""),
            name: meta.name.as_deref().unwrap_or(""),
            relationship: Some(meta.relationship.as_deref().unwrap_or("")),
            lang: meta.lang_or_default(),
        },
    )
    .await
    {
        Ok(outcome) => {
            if outcome.degraded {
                degraded.push("music_prompt".to_string());
            }
            outcome.enriched
        }
        Err(e) => {
            tracing::warn!(
                gift_id,
                stage = "music_prompt",
                error = %e,
                "music prompt enrichment failed; using raw style fallback"
            );
            degraded.push("music_prompt".to_string());
            EnrichedPrompt::fallback(style)
        }
    };

    let submission = GenSubmission {
        lyrics,
        style: style.to_string(),
        title: meta.title_or_default().to_string(),
        vocal: meta.vocal_or_default().to_string(),
        kind: gift.kind.clone(),
        provider: provider.to_string(),
    };

    // Check the style that will actually be submitted for artist names —
    // this used to check `enriched.prompt`, which the Suno custom-mode path
    // never sends. For instrumental gifts the enriched prompt IS the
    // submitted prompt, so check it as well.
    for w in &crate::tools::lyrics_validator::check_style_prompt(&submission.style) {
        tracing::warn!(gift_id, stage = "style_check", warning = %w, "style check warning");
    }
    if submission.kind == "instrumental" {
        for w in &crate::tools::lyrics_validator::check_style_prompt(&enriched.prompt) {
            tracing::warn!(gift_id, stage = "style_check", warning = %w, "instrumental prompt check warning");
        }
    }

    let resp = submit(gen_task, store, gift_id, &submission, &enriched).await?;

    // Record degraded stages on the gift so the gift page can show that
    // quality steps were skipped this run. Best-effort: the job is already
    // submitted, a bookkeeping failure must not fail the request.
    if let Err(e) = store.set_meta_degraded(gift_id, &degraded) {
        tracing::warn!(gift_id, stage = "degraded", error = %e, "recording degraded stages failed");
    }
    Ok(resp)
}

/// Build [`GenRequest::params`] for the given provider, checking every key
/// against what that provider's dialect actually forwards (previously seven
/// enrichment keys — genre/tempo/mood/vocal_style/instrumentation/production/
/// exclude — went out for every provider and were silently dropped):
///
/// - **suno**: `lyrics`/`instrumental` are handled explicitly by the dialect;
///   the passthrough whitelist is `style`/`title`/`negativeTags`/`vocalGender`/
///   `styleWeight`/`weirdnessConstraint`/`audioWeight`/`personaId`/
///   `personaModel` (crates/orchest-provider-http/src/gen/suno.rs
///   `PASSTHROUGH_PARAMS`).
/// - **mureka**: passthrough is `lyrics`/`n`/`reference_id`/`vocal_id`/
///   `melody_id`/`gender`; `is_instrumental` only selects the endpoint.
/// - **minimax**: forwards every params key verbatim onto its API body, so
///   only keys its API actually understands are sent.
fn build_gen_params(submission: &GenSubmission, enriched: &EnrichedPrompt) -> Value {
    let instrumental = submission.kind == "instrumental";
    let mut params = serde_json::Map::new();
    match submission.provider.as_str() {
        "suno" => {
            params.insert("lyrics".into(), json!(submission.lyrics));
            params.insert("style".into(), json!(submission.style));
            params.insert("title".into(), json!(submission.title));
            if instrumental {
                params.insert("instrumental".into(), json!(true));
            } else {
                // sunoapi.org vocalGender enum is "m"/"f" — the meta value is
                // "male"/"female". No vocalGender for instrumentals: there
                // are no vocals to gender.
                let vocal_gender = match submission.vocal.as_str() {
                    "male" => Some("m"),
                    "female" => Some("f"),
                    _ => None,
                };
                if let Some(g) = vocal_gender {
                    params.insert("vocalGender".into(), json!(g));
                }
            }
            let exclude = enriched.exclude.trim();
            if !exclude.is_empty() {
                params.insert("negativeTags".into(), json!(exclude));
            }
        }
        "mureka" | "minimax" => {
            params.insert("lyrics".into(), json!(submission.lyrics));
            if instrumental {
                params.insert("is_instrumental".into(), json!(true));
            }
        }
        _ => {
            // Unknown provider: the conservative common denominator —
            // lyrics/style/title are meaningful on every dialect above.
            params.insert("lyrics".into(), json!(submission.lyrics));
            params.insert("style".into(), json!(submission.style));
            params.insert("title".into(), json!(submission.title));
        }
    }
    Value::Object(params)
}

/// Submit a music generation job for the gift and persist the handle.
pub async fn submit(
    gen_task: &Arc<dyn GenTask>,
    store: &GiftStore,
    gift_id: &str,
    submission: &GenSubmission,
    enriched: &EnrichedPrompt,
) -> AppResult<GenerateResponse> {
    let params = build_gen_params(submission, enriched);

    let gen_req = GenRequest {
        prompt: enriched.prompt.clone(),
        params,
    };
    let handle = gen_task.submit(gen_req).await?;
    let handle_json = serde_json::to_string(&handle)?;

    store.update_gen(gift_id, &handle_json, "pending")?;

    Ok(GenerateResponse {
        id: gift_id.to_string(),
        status: "pending".to_string(),
        handle: Some(handle_json),
    })
}

/// Poll for the generation job status and fetch assets when done.
///
/// Same logic as the original `generate_status` handler.
pub async fn poll(
    gen_task: &Arc<dyn GenTask>,
    store: &GiftStore,
    gift_id: &str,
) -> AppResult<GenStatusResponse> {
    let gift = store.get(gift_id)?;
    if gift.gen_status.as_deref() == Some("done") {
        return Ok(GenStatusResponse {
            id: gift_id.to_string(),
            status: "done".to_string(),
            audio_url: gift.audio_url,
        });
    }

    let handle_json = match &gift.gen_handle {
        Some(h) => h.clone(),
        None => return Err(AppError::BadRequest("generation not submitted".to_string())),
    };

    let handle: GenHandle = serde_json::from_str(&handle_json)
        .map_err(|e| AppError::BadRequest(format!("invalid gen handle: {e}")))?;

    let status = match gen_task.poll(&handle).await {
        Ok(s) => s,
        Err(e) => return Err(AppError::Gen(e.to_string())),
    };

    if status == GenStatus::Done {
        match handle_done(
            gen_task.as_ref(),
            store,
            gift_id,
            gift.lyrics.as_deref(),
            &handle,
        )
        .await
        {
            Ok(audio_url) => {
                return Ok(GenStatusResponse {
                    id: gift_id.to_string(),
                    status: "done".to_string(),
                    audio_url,
                });
            }
            Err(e) => {
                store.mark_gen_failed(gift_id)?;
                return Err(e);
            }
        }
    }

    let status_str = match status {
        GenStatus::Pending => "pending",
        GenStatus::Running => "running",
        GenStatus::Done => unreachable!(),
        GenStatus::Failed => "failed",
    };

    if status == GenStatus::Failed {
        store.mark_gen_failed(gift_id)?;
    }

    Ok(GenStatusResponse {
        id: gift_id.to_string(),
        status: status_str.to_string(),
        audio_url: None,
    })
}

/// Stream SSE generation status events until the job completes or the SSE
/// window (48 × 5s ≈ 4 minutes) elapses.
///
/// Same logic as the original `generate_stream` handler (poll loop +
/// SSE wrapping). Re-fetches the gift itself: it runs after the HTTP
/// response, in its own spawned task. A timeout does not abandon the job —
/// a background task keeps polling to a terminal state and finalizes the
/// gift (`handle_done` / `mark_gen_failed`), since after the client is gone
/// nobody else would.
pub fn stream(
    gen_task: Arc<dyn GenTask>,
    store: GiftStore,
    gift_id: String,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(8);

    tokio::spawn(async move {
        let gift = match store.get(&gift_id) {
            Ok(g) => g,
            Err(_) => {
                let _ = tx
                    .send(Ok(Event::default().data("{\"status\":\"error\"}")))
                    .await;
                return;
            }
        };
        let handle: GenHandle = match gift.gen_handle.as_deref() {
            Some(json) => match serde_json::from_str(json) {
                Ok(h) => h,
                Err(_) => {
                    let _ = tx
                        .send(Ok(Event::default().data("{\"status\":\"error\"}")))
                        .await;
                    return;
                }
            },
            None => {
                let _ = tx
                    .send(Ok(Event::default().data("{\"status\":\"error\"}")))
                    .await;
                return;
            }
        };
        let lyrics = gift.lyrics;

        let _ = tx
            .send(Ok(Event::default().data("{\"status\":\"pending\"}")))
            .await;

        for _ in 0..POLL_ROUNDS {
            sleep(POLL_INTERVAL).await;
            match gen_task.poll(&handle).await {
                Ok(GenStatus::Done) => {
                    match handle_done(
                        gen_task.as_ref(),
                        &store,
                        &gift_id,
                        lyrics.as_deref(),
                        &handle,
                    )
                    .await
                    {
                        Ok(audio_url) => {
                            let _ = tx
                                .send(Ok(Event::default().data(
                                    json!({"status":"done","audio_url":audio_url}).to_string(),
                                )))
                                .await;
                        }
                        Err(_) => {
                            let _ = store.mark_gen_failed(&gift_id);
                            let _ = tx
                                .send(Ok(Event::default().data("{\"status\":\"failed\"}")))
                                .await;
                        }
                    }
                    return;
                }
                Ok(GenStatus::Failed) => {
                    let _ = store.mark_gen_failed(&gift_id);
                    let _ = tx
                        .send(Ok(Event::default().data("{\"status\":\"failed\"}")))
                        .await;
                    return;
                }
                Ok(GenStatus::Running) => {
                    let _ = tx
                        .send(Ok(Event::default().data("{\"status\":\"running\"}")))
                        .await;
                }
                _ => {} // pending or error, keep polling
            }
        }
        let _ = tx
            .send(Ok(Event::default().data("{\"status\":\"timeout\"}")))
            .await;

        // The SSE window elapsed without a terminal status, but the provider
        // job may still land — keep polling in the background so the gift is
        // finalized rather than stuck on `pending` with the song lost.
        tokio::spawn(finalize_in_background(
            gen_task, store, gift_id, lyrics, handle,
        ));
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

/// Poll cadence shared by the SSE loop and the background finalizer.
const POLL_ROUNDS: u32 = 48; // 48 × 5s ≈ 4 minutes
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Keep polling a timed-out job in the background until it reaches a
/// terminal state, then finalize the gift. Runs detached: failures are
/// logged and the gift is marked failed, never propagated.
async fn finalize_in_background(
    gen_task: Arc<dyn GenTask>,
    store: GiftStore,
    gift_id: String,
    lyrics: Option<String>,
    handle: GenHandle,
) {
    for _ in 0..POLL_ROUNDS {
        sleep(POLL_INTERVAL).await;
        match gen_task.poll(&handle).await {
            Ok(GenStatus::Done) => {
                if let Err(e) = handle_done(
                    gen_task.as_ref(),
                    &store,
                    &gift_id,
                    lyrics.as_deref(),
                    &handle,
                )
                .await
                {
                    eprintln!("[music-gift] background finalize failed [{gift_id}]: {e}");
                    let _ = store.mark_gen_failed(&gift_id);
                }
                return;
            }
            Ok(GenStatus::Failed) => {
                let _ = store.mark_gen_failed(&gift_id);
                return;
            }
            _ => {} // pending/running or a transient poll error — keep waiting
        }
    }
    // Still no terminal status after another ~4 minutes: give up rather than
    // leave the gift pending forever.
    eprintln!("[music-gift] background poll exhausted [{gift_id}]");
    let _ = store.mark_gen_failed(&gift_id);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Fetch completed generation assets, extract the first audio URL, generate
/// synced lyrics (LRC), and update the store.
async fn handle_done(
    gen_task: &dyn GenTask,
    store: &GiftStore,
    gift_id: &str,
    lyrics: Option<&str>,
    handle: &GenHandle,
) -> AppResult<Option<String>> {
    let result = gen_task
        .fetch(handle)
        .await
        .map_err(|e| AppError::Gen(e.to_string()))?;

    let url = result.assets.first().and_then(|a| match a {
        GenAsset::Url { url, .. } => Some(url.clone()),
        _ => None,
    });

    if let Some(ref u) = url {
        store.update_audio(gift_id, u)?;
    }

    // Synced lyrics for the scrolling viewer. Prefer the provider's forced
    // alignment (structured TimedText → rendered to LRC); fall back to our own
    // text estimate from lyrics + reported duration only when the provider gave
    // no timed text. Best-effort — never fail generation over it.
    let duration = result
        .diagnostic_metadata
        .get("duration_secs")
        .and_then(serde_json::Value::as_f64);
    let lrc = result
        .timed_text
        .as_ref()
        .and_then(crate::lrc::timed_text_to_lrc)
        .or_else(|| match (lyrics, duration) {
            (Some(text), Some(dur)) if dur > 0.0 => crate::lrc::generate_lrc(text, dur),
            _ => None,
        });
    if let Some(lrc) = lrc {
        if let Err(e) = store.update_lrc(gift_id, &lrc, duration) {
            eprintln!("[music-gift] lrc store failed [{gift_id}]: {e}");
        }
    }

    Ok(url)
}
// ── Music Prompt Generation ──────────────────────────────────────────────────

/// Outcome of the enrichment LLM call: the prompt plus a degradation flag.
/// `degraded` is true when the model's answer could not be parsed and the
/// raw-style fallback is returned instead of real enrichment — callers
/// surface this to the user instead of failing silently.
pub struct Enrichment {
    pub enriched: EnrichedPrompt,
    pub degraded: bool,
}

/// Generate a structured, enriched music-generation prompt using the provider skill.
///
/// Returns an `EnrichedPrompt` with all six style dimensions plus exclude tags,
/// not just the flat Suno prompt string.
/// Map a UI language code to a name the prompt model will recognise.
fn lang_name(lang: &str) -> &str {
    match lang {
        "zh" => "Chinese (Mandarin)",
        "fr" => "French",
        "es" => "Spanish",
        "ru" => "Russian",
        _ => "English",
    }
}

/// Everything the provider skill template needs to build a music prompt.
/// Grouped into a struct so the fields travel together (and to keep the
/// function within a sane argument count).
pub struct MusicPromptInput<'a> {
    pub provider: &'a str,
    pub lyrics: &'a str,
    pub style: &'a str,
    pub title: &'a str,
    pub vocal: &'a str,
    pub scene: &'a str,
    pub name: &'a str,
    pub relationship: Option<&'a str>,
    pub lang: &'a str,
}

pub async fn generate_music_prompt(
    chat_model: Arc<dyn ChatModel>,
    input: &MusicPromptInput<'_>,
) -> AppResult<Enrichment> {
    let skill_template = MUSIC_PROMPT_SKILLS.get(input.provider).ok_or_else(|| {
        AppError::BadRequest(format!("unknown music provider: {}", input.provider))
    })?;

    let relationship = input.relationship.unwrap_or("friend");

    let user_message = skill_template
        .replace("{lyrics}", input.lyrics)
        .replace("{style}", input.style)
        .replace("{title}", input.title)
        .replace("{vocal}", input.vocal)
        .replace("{scene}", input.scene)
        .replace("{name}", input.name)
        .replace("{relationship}", relationship)
        .replace("{lang}", lang_name(input.lang));

    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text(user_message)],
    }];

    let options = RequestOptions::default();

    let response = chat_model
        .complete(&messages, &[], &options, None)
        .await
        .map_err(|e| AppError::Llm(e.to_string()))?;

    let text: String = response
        .content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(t) = block {
                Some(t.clone())
            } else {
                None
            }
        })
        .collect();

    let json_str = serde_json::from_str::<Value>(&text)
        .ok()
        .map(|_| text.clone())
        .or_else(|| extract_json_block(&text));

    if let Some(json_str) = json_str {
        if let Ok(parsed) = serde_json::from_str::<Value>(&json_str) {
            let prompt = parsed
                .get("prompt")
                .or_else(|| parsed.get("base_prompt"))
                .and_then(Value::as_str)
                .unwrap_or(&text)
                .to_string();

            let genre = parsed.get("genre").and_then(json_array).unwrap_or_default();
            let tempo = parsed
                .get("tempo")
                .and_then(str_or_empty)
                .unwrap_or_default();
            let mood = parsed.get("mood").and_then(json_array).unwrap_or_default();
            let vocal_style = parsed
                .get("vocal_style")
                .and_then(str_or_empty)
                .unwrap_or_else(|| input.vocal.to_string());
            let instrumentation = parsed
                .get("instrumentation")
                .and_then(str_or_empty)
                .unwrap_or_default();
            let production = parsed
                .get("production")
                .and_then(str_or_empty)
                .unwrap_or_default();
            let exclude = parsed
                .get("exclude")
                .and_then(str_or_empty)
                .unwrap_or_default();
            let style_tags = parsed
                .get("style_tags")
                .and_then(json_array)
                .unwrap_or_default();

            return Ok(Enrichment {
                enriched: EnrichedPrompt {
                    prompt,
                    genre,
                    tempo,
                    mood,
                    vocal_style,
                    instrumentation,
                    production,
                    exclude,
                    style_tags,
                },
                degraded: false,
            });
        }
    }

    // The model answered with prose instead of the JSON the template asks
    // for. This used to fall back silently; log it and mark the outcome
    // degraded so the caller can surface it.
    tracing::warn!(
        stage = "music_prompt",
        provider = input.provider,
        raw_len = text.len(),
        "music prompt enrichment output was not parseable JSON; using raw style fallback"
    );
    Ok(Enrichment {
        enriched: EnrichedPrompt::fallback(input.style),
        degraded: true,
    })
}

fn json_array(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    })
}

fn str_or_empty(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

/// Extract the content of the first ```json ... ``` fenced code block.
fn extract_json_block(text: &str) -> Option<String> {
    let start_marker = "```json\n";
    let start = text.find(start_marker)? + start_marker.len();
    let rest = &text[start..];
    let end = rest.find("\n```")?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submission_for(provider: &str) -> GenSubmission {
        GenSubmission {
            lyrics: "[verse]\nla la".to_string(),
            style: "warm acoustic".to_string(),
            title: "Wheels".to_string(),
            vocal: "male".to_string(),
            kind: "song".to_string(),
            provider: provider.to_string(),
        }
    }

    fn enriched() -> EnrichedPrompt {
        EnrichedPrompt {
            prompt: "male, breathy, indie folk, warm nostalgia".to_string(),
            genre: vec!["indie folk".to_string()],
            tempo: "ballad-slow".to_string(),
            mood: vec!["warm".to_string()],
            vocal_style: "male, breathy".to_string(),
            instrumentation: "acoustic guitar".to_string(),
            production: "spacious reverb".to_string(),
            exclude: "no backing vocals".to_string(),
            style_tags: vec!["indie folk".to_string()],
        }
    }

    /// Every key sent for Suno must be one the dialect actually forwards:
    /// `lyrics`/`instrumental` are handled explicitly, the rest must be on
    /// the passthrough whitelist (crates/orchest-provider-http/src/gen/
    /// suno.rs `PASSTHROUGH_PARAMS`). The old seven enrichment keys
    /// (genre/tempo/mood/vocal_style/instrumentation/production/exclude)
    /// were silently dropped and must not come back.
    #[test]
    fn suno_params_carry_whitelisted_keys_only() {
        let params = build_gen_params(&submission_for("suno"), &enriched());
        let mut keys: Vec<&str> = params
            .as_object()
            .expect("params object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["lyrics", "negativeTags", "style", "title", "vocalGender"]
        );

        assert_eq!(params["style"], "warm acoustic");
        assert_eq!(params["vocalGender"], "m"); // meta "male" -> sunoapi enum "m"
        assert_eq!(params["negativeTags"], "no backing vocals");
        assert!(params.get("instrumental").is_none());
        for key in [
            "genre",
            "tempo",
            "mood",
            "vocal_style",
            "instrumentation",
            "production",
            "exclude",
        ] {
            assert!(params.get(key).is_none(), "{key} leaked into suno params");
        }
    }

    #[test]
    fn suno_params_map_female_vocal() {
        let mut sub = submission_for("suno");
        sub.vocal = "female".to_string();
        let params = build_gen_params(&sub, &enriched());
        assert_eq!(params["vocalGender"], "f");
    }

    #[test]
    fn suno_params_instrumental_skips_vocal_gender() {
        let mut sub = submission_for("suno");
        sub.kind = "instrumental".to_string();
        sub.lyrics = String::new();
        let params = build_gen_params(&sub, &enriched());
        assert_eq!(params["instrumental"], true);
        assert!(params.get("vocalGender").is_none());
        // negativeTags still apply to an instrumental generation.
        assert_eq!(params["negativeTags"], "no backing vocals");
    }

    #[test]
    fn suno_params_omit_empty_negative_tags() {
        let mut e = enriched();
        e.exclude = "  ".to_string();
        let params = build_gen_params(&submission_for("suno"), &e);
        assert!(params.get("negativeTags").is_none());
    }

    /// Mureka (whitelist passthrough) and minimax (verbatim passthrough)
    /// must not see the Suno keys — only `lyrics`, plus the instrumental
    /// selector for instrumental gifts. This keeps the song-path wire
    /// identical to before the D1 alignment.
    #[test]
    fn mureka_and_minimax_params_are_lyrics_only() {
        for provider in ["mureka", "minimax"] {
            let params = build_gen_params(&submission_for(provider), &enriched());
            let keys: Vec<&str> = params
                .as_object()
                .expect("params object")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, ["lyrics"], "{provider} params: {keys:?}");

            let mut sub = submission_for(provider);
            sub.kind = "instrumental".to_string();
            let params = build_gen_params(&sub, &enriched());
            assert_eq!(params["is_instrumental"], true, "{provider}");
            assert!(
                params.get("vocalGender").is_none() && params.get("negativeTags").is_none(),
                "{provider} must not see suno-only keys"
            );
        }
    }
}
