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
    ChatModel, ContentBlock, GenAsset, GenAssetRole, GenHandle, GenRequest, GenStatus, GenTask,
    Message, MusicParams, RequestOptions, Role, VocalGender,
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
/// [`GenRequest::music`] is built from. `submit` takes these instead of
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

/// Build the typed [`MusicParams`] for the given provider, mapping the gift
/// and enrichment fields onto the music knobs each dialect actually consumes
/// (the submit path is fully typed — raw `GenRequest.params` stays empty, so
/// a misspelled key is a compile error, not a silent drop):
///
/// - **suno**: all knobs typed. `style` folds the raw style together with the
///   enrichment's genre/tempo/mood/instrumentation/production — in Suno's
///   custom mode the prompt slot carries the lyrics, so the style string is
///   the only place the LLM rewrite reaches the wire (the prompt template
///   already builds these dimensions as the compact descriptor line).
///   `exclude` → `negative_tags`. `vocal_gender` keeps coming from the gift
///   meta ("male"/"female" → [`VocalGender`]), not from `vocal_style`: the
///   enrichment's vocal_style is free text ("male, breathy"), not a valid
///   gender value.
/// - **mureka / minimax**: `lyrics` + `instrumental` only — those dialects
///   map them onto `lyrics`/`is_instrumental`; style and the other knobs
///   ride in the prompt string, as before.
/// - **unknown provider**: the conservative typed denominator —
///   lyrics/style/title are meaningful on every dialect above.
fn build_music_params(submission: &GenSubmission, enriched: &EnrichedPrompt) -> MusicParams {
    let instrumental = submission.kind == "instrumental";
    // Empty lyrics are "unset", not an empty string on the wire.
    let lyrics = (!submission.lyrics.is_empty()).then(|| submission.lyrics.clone());
    match submission.provider.as_str() {
        "suno" => {
            let style = composed_style(submission, enriched);
            let exclude = enriched.exclude.trim();
            let vocal_gender = if instrumental {
                // No vocal gender for instrumentals: there are no vocals to
                // gender.
                None
            } else {
                // sunoapi.org's vocalGender enum is "m"/"f" — the meta value
                // is "male"/"female".
                match submission.vocal.as_str() {
                    "male" => Some(VocalGender::Male),
                    "female" => Some(VocalGender::Female),
                    other => {
                        tracing::warn!(
                            vocal = %other,
                            "unexpected vocal value, vocal_gender omitted from Suno music params"
                        );
                        None
                    }
                }
            };
            MusicParams {
                lyrics,
                instrumental: instrumental.then_some(true),
                style: Some(style),
                title: Some(submission.title.clone()),
                negative_tags: (!exclude.is_empty()).then(|| exclude.to_string()),
                vocal_gender,
                ..MusicParams::default()
            }
        }
        "mureka" | "minimax" => MusicParams {
            lyrics,
            instrumental: instrumental.then_some(true),
            ..MusicParams::default()
        },
        _ => MusicParams {
            lyrics,
            style: Some(submission.style.clone()),
            title: Some(submission.title.clone()),
            ..MusicParams::default()
        },
    }
}

/// Fold the raw style and the enrichment's structured dimensions
/// (genre/tempo/mood/instrumentation/production) into the single style string
/// the Suno `style` knob expects. Empty parts drop out; the raw style leads.
fn composed_style(submission: &GenSubmission, enriched: &EnrichedPrompt) -> String {
    std::iter::once(submission.style.as_str())
        .chain(enriched.genre.iter().map(String::as_str))
        .chain(std::iter::once(enriched.tempo.as_str()))
        .chain(enriched.mood.iter().map(String::as_str))
        .chain(std::iter::once(enriched.instrumentation.as_str()))
        .chain(std::iter::once(enriched.production.as_str()))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Submit a music generation job for the gift and persist the handle.
pub async fn submit(
    gen_task: &Arc<dyn GenTask>,
    store: &GiftStore,
    gift_id: &str,
    submission: &GenSubmission,
    enriched: &EnrichedPrompt,
) -> AppResult<GenerateResponse> {
    let gen_req = GenRequest {
        prompt: enriched.prompt.clone(),
        // Fully typed submit path: no raw dialect extras, so `params` stays
        // empty and every knob travels in `music`.
        params: Value::Null,
        music: Some(build_music_params(submission, enriched)),
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

/// Fetch completed generation assets, extract the primary audio URL, persist
/// the provider's cover art, generate synced lyrics (LRC), and update the
/// store.
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

    // The audio track is the Primary-role asset: cover art rides alongside in
    // the same asset list now, so pick by role, never by position.
    let url = result.assets.iter().find_map(|a| match a {
        GenAsset::Url {
            url,
            role: GenAssetRole::Primary,
            ..
        } => Some(url.clone()),
        _ => None,
    });

    if let Some(ref u) = url {
        store.update_audio(gift_id, u)?;
    }

    // Cover art the provider surfaced as a Cover-role asset (Suno). Decorative
    // — a store failure must not fail the generation.
    let cover_url = result.assets.iter().find_map(|a| match a {
        GenAsset::Url {
            url,
            role: GenAssetRole::Cover,
            ..
        } => Some(url.clone()),
        _ => None,
    });
    if let Some(ref c) = cover_url {
        if let Err(e) = store.update_cover(gift_id, c) {
            eprintln!("[music-gift] cover store failed [{gift_id}]: {e}");
        }
    }

    // Synced lyrics for the scrolling viewer. Prefer the provider's forced
    // alignment (structured TimedText → rendered to LRC); fall back to our own
    // text estimate from lyrics + the typed duration only when the provider
    // gave no timed text. Best-effort — never fail generation over it.
    let duration = result.duration_secs;
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

    use orchest_protocol::{Capability, CapabilityDescriptor, GenResult, Modality, ProtocolError};

    use crate::gift::{Gift, GiftStore};

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

    /// Every knob sent for Suno must be one the dialect actually consumes —
    /// now a compile-time property: `MusicParams` only has the typed fields
    /// (crates/orchest-protocol `MusicParams`) the suno dialect maps
    /// (crates/orchest-provider-http/src/gen/suno.rs). The old seven raw
    /// enrichment keys (genre/tempo/mood/vocal_style/instrumentation/
    /// production/exclude) were silently dropped and must not come back —
    /// they now fold into `style`/`negative_tags` instead.
    #[test]
    fn suno_music_params_map_onto_typed_knobs() {
        let music = build_music_params(&submission_for("suno"), &enriched());
        assert_eq!(music.lyrics.as_deref(), Some("[verse]\nla la"));
        assert_eq!(music.title.as_deref(), Some("Wheels"));
        assert_eq!(music.vocal_gender, Some(VocalGender::Male)); // meta "male" -> sunoapi "m"
        assert_eq!(music.negative_tags.as_deref(), Some("no backing vocals"));
        assert_eq!(music.instrumental, None);
        // The raw style leads, then the enrichment's genre/tempo/mood/
        // instrumentation/production — the style string is the only place
        // those dimensions reach the Suno custom-mode wire.
        assert_eq!(
            music.style.as_deref(),
            Some("warm acoustic, indie folk, ballad-slow, warm, acoustic guitar, spacious reverb")
        );
    }

    #[test]
    fn suno_style_string_is_raw_style_when_enrichment_is_empty() {
        // The degraded/fallback path contributes no dimensions, so the style
        // is exactly the gift's raw style (no dangling separators).
        let music = build_music_params(
            &submission_for("suno"),
            &EnrichedPrompt::fallback("warm acoustic"),
        );
        assert_eq!(music.style.as_deref(), Some("warm acoustic"));
    }

    #[test]
    fn suno_params_map_female_vocal() {
        let mut sub = submission_for("suno");
        sub.vocal = "female".to_string();
        let music = build_music_params(&sub, &enriched());
        assert_eq!(music.vocal_gender, Some(VocalGender::Female));
    }

    #[test]
    fn suno_params_instrumental_skips_vocal_gender() {
        let mut sub = submission_for("suno");
        sub.kind = "instrumental".to_string();
        sub.lyrics = String::new();
        let music = build_music_params(&sub, &enriched());
        assert_eq!(music.instrumental, Some(true));
        assert_eq!(music.vocal_gender, None);
        // Empty lyrics are unset, not an empty string on the wire.
        assert_eq!(music.lyrics, None);
        // negative_tags still apply to an instrumental generation.
        assert_eq!(music.negative_tags.as_deref(), Some("no backing vocals"));
    }

    #[test]
    fn suno_params_omit_empty_negative_tags() {
        let mut e = enriched();
        e.exclude = "  ".to_string();
        let music = build_music_params(&submission_for("suno"), &e);
        assert_eq!(music.negative_tags, None);
    }

    /// Mureka and minimax consume only `lyrics`/`instrumental` — style,
    /// title, and the Suno-only knobs must stay unset for them. This keeps
    /// the song-path wire identical to before the typing.
    #[test]
    fn mureka_and_minimax_params_are_lyrics_only() {
        for provider in ["mureka", "minimax"] {
            let music = build_music_params(&submission_for(provider), &enriched());
            assert_eq!(
                music.lyrics.as_deref(),
                Some("[verse]\nla la"),
                "{provider}"
            );
            assert_eq!(music.instrumental, None, "{provider}");
            assert_eq!(music.style, None, "{provider}");
            assert_eq!(music.title, None, "{provider}");
            assert_eq!(music.vocal_gender, None, "{provider}");
            assert_eq!(music.negative_tags, None, "{provider}");

            let mut sub = submission_for(provider);
            sub.kind = "instrumental".to_string();
            sub.lyrics = String::new();
            let music = build_music_params(&sub, &enriched());
            assert_eq!(music.instrumental, Some(true), "{provider}");
            assert_eq!(music.lyrics, None, "{provider}");
        }
    }

    /// Minimal [`GenTask`] fake for `handle_done`: submit/poll are never
    /// reached; `fetch` returns the canned result.
    struct FakeGenTask {
        result: GenResult,
    }

    #[async_trait::async_trait]
    impl GenTask for FakeGenTask {
        fn provider_name(&self) -> &str {
            "fake"
        }

        fn model_name(&self) -> &str {
            "fake-model"
        }

        fn descriptor(&self) -> CapabilityDescriptor {
            CapabilityDescriptor::new("fake", "fake-model", Capability::GenTask)
                .with_input_modalities([Modality::Text])
                .with_output_modalities([Modality::Audio])
        }

        async fn submit(&self, _req: GenRequest) -> Result<GenHandle, ProtocolError> {
            unimplemented!("handle_done only calls fetch")
        }

        async fn poll(&self, _handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
            unimplemented!("handle_done only calls fetch")
        }

        async fn fetch(&self, _handle: &GenHandle) -> Result<GenResult, ProtocolError> {
            Ok(self.result.clone())
        }
    }

    /// A store backed by a tempdir SQLite file with one gift inserted. The
    /// TempDir is returned so the caller keeps it alive for the store's
    /// lifetime (SQLite needs the directory for its journal files).
    fn store_with_gift(gift_id: &str, lyrics: Option<String>) -> (GiftStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gifts.db").to_string_lossy().into_owned();
        let store = GiftStore::open(&db).expect("open store");
        store
            .create(&Gift {
                id: gift_id.to_string(),
                kind: "song".to_string(),
                lyrics,
                meta: json!({}),
                audio_url: None,
                cover_url: None,
                photos: vec![],
                gen_handle: None,
                gen_status: None,
                countdown_status: None,
                lrc: None,
                duration_secs: None,
                creator_token: "tok".to_string(),
                published: false,
                likes: vec![],
                created_at: "0".to_string(),
                published_at: None,
            })
            .expect("create gift");
        (store, dir)
    }

    /// `handle_done` consumes the typed surface only: the audio URL comes from
    /// the Primary-role asset even when the cover sorts first, the Cover-role
    /// asset is persisted on the gift, and the LRC fallback runs off the typed
    /// `duration_secs` — nothing reads diagnostic_metadata anymore.
    #[tokio::test]
    async fn handle_done_consumes_roles_and_typed_duration() {
        let (store, _dir) = store_with_gift("g1", Some("[Verse]\nline one\nline two".to_string()));
        let task = FakeGenTask {
            result: GenResult {
                assets: vec![
                    // Cover listed first: the pick must be by role, not position.
                    GenAsset::Url {
                        url: "https://cdn/cover.jpeg".to_string(),
                        media_type: Some("image/jpeg".to_string()),
                        role: GenAssetRole::Cover,
                    },
                    GenAsset::Url {
                        url: "https://cdn/track.mp3".to_string(),
                        media_type: Some("audio/mpeg".to_string()),
                        role: GenAssetRole::Primary,
                    },
                ],
                diagnostic_metadata: json!({ "provider": "suno" }),
                timed_text: None,
                duration_secs: Some(31.84),
            },
        };
        let handle = GenHandle {
            id: "task-1".to_string(),
            provider: Some("fake".to_string()),
        };

        let url = handle_done(
            &task,
            &store,
            "g1",
            Some("[Verse]\nline one\nline two"),
            &handle,
        )
        .await
        .expect("handle_done");
        assert_eq!(url.as_deref(), Some("https://cdn/track.mp3"));

        let got = store.get("g1").expect("get gift");
        assert_eq!(got.audio_url.as_deref(), Some("https://cdn/track.mp3"));
        assert_eq!(got.cover_url.as_deref(), Some("https://cdn/cover.jpeg"));
        assert_eq!(got.duration_secs, Some(31.84));
        // The estimate fallback produced LRC from lyrics + typed duration.
        let lrc = got.lrc.expect("lrc stored");
        assert!(lrc.contains("line one"), "lrc: {lrc}");
    }

    /// Without a typed duration (and no timed text) there is no LRC fallback,
    /// and a result without a Cover asset leaves the gift's cover unset.
    #[tokio::test]
    async fn handle_done_without_duration_or_cover_stores_neither() {
        let (store, _dir) = store_with_gift("g2", Some("[Verse]\nline one".to_string()));
        let task = FakeGenTask {
            result: GenResult {
                assets: vec![GenAsset::Url {
                    url: "https://cdn/track.mp3".to_string(),
                    media_type: Some("audio/mpeg".to_string()),
                    role: GenAssetRole::Primary,
                }],
                diagnostic_metadata: json!({ "provider": "minimax" }),
                timed_text: None,
                duration_secs: None,
            },
        };
        let handle = GenHandle {
            id: "task-2".to_string(),
            provider: Some("fake".to_string()),
        };

        handle_done(&task, &store, "g2", Some("[Verse]\nline one"), &handle)
            .await
            .expect("handle_done");

        let got = store.get("g2").expect("get gift");
        assert_eq!(got.audio_url.as_deref(), Some("https://cdn/track.mp3"));
        assert_eq!(got.cover_url, None);
        assert_eq!(got.duration_secs, None);
        assert_eq!(got.lrc, None);
    }
}
