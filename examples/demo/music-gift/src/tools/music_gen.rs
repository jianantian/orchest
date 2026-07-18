//! Music generation tool wrapping `GenTask` for submit/poll/stream.
//!
//! Extracted from the route handlers so routes stay thin. Logic is copied
//! exactly from the original handlers — behavior is unchanged.

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
use crate::prompts::MUSIC_PROMPT_SKILLS;
use crate::gift::GiftStore;

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

// ── Tool ────────────────────────────────────────────────────────────────────

/// Thin wrapper around `Arc<dyn GenTask>` with the submit/poll/stream lifecycle
/// matching the `/api/generate/*` handlers exactly.
pub struct MusicGenTool {
    gen_task: Arc<dyn GenTask>,
}

impl MusicGenTool {
    pub fn new(gen_task: Arc<dyn GenTask>) -> Self {
        Self { gen_task }
    }

    /// Submit a music generation job for the gift and persist the handle.
    ///
    pub async fn submit(&self, store: &GiftStore, gift_id: &str, enriched: &EnrichedPrompt) -> AppResult<GenerateResponse> {
        let gift = store.get(gift_id)?;

        let style = gift
            .meta
            .get("style")
            .and_then(Value::as_str)
            .unwrap_or("healing and warm");
        let lyrics = gift.lyrics.unwrap_or_default();
        let title = gift
            .meta
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Gift Song");
        let params = json!({
            "lyrics": lyrics,
            "style": style,
            "title": title,
            "genre": enriched.genre,
            "tempo": enriched.tempo,
            "mood": enriched.mood,
            "vocal_style": enriched.vocal_style,
            "instrumentation": enriched.instrumentation,
            "production": enriched.production,
            "exclude": enriched.exclude,
        });

        let gen_req = GenRequest { prompt: enriched.prompt.clone(), params };
        let handle = self.gen_task.submit(gen_req).await?;
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
    pub async fn poll(&self, store: &GiftStore, gift_id: &str) -> AppResult<GenStatusResponse> {
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
            None => {
                return Err(AppError::BadRequest(
                    "generation not submitted".to_string(),
                ))
            }
        };

        let handle: GenHandle = serde_json::from_str(&handle_json)
            .map_err(|e| AppError::BadRequest(format!("invalid gen handle: {e}")))?;

        let status = match self.gen_task.poll(&handle).await {
            Ok(s) => s,
            Err(e) => return Err(AppError::Gen(e.to_string())),
        };

        if status == GenStatus::Done {
            match handle_done(
                self.gen_task.as_ref(),
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

    /// Stream SSE generation status events until the job completes or times out.
    ///
    /// Same logic as the original `generate_stream` handler (poll loop +
    /// SSE wrapping).
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
            let lyrics = gift.lyrics.as_deref();

            let _ = tx
                .send(Ok(Event::default().data("{\"status\":\"pending\"}")))
                .await;

            for _ in 0..48 {
                // poll up to 4 minutes
                sleep(Duration::from_secs(5)).await;
                match gen_task.poll(&handle).await {
                    Ok(GenStatus::Done) => {
                        match handle_done(
                            gen_task.as_ref(),
                            &store,
                            &gift_id,
                            lyrics,
                            &handle,
                        )
                        .await
                        {
                            Ok(audio_url) => {
                                let _ = tx
                                    .send(Ok(Event::default().data(
                                        json!({"status":"done","audio_url":audio_url})
                                            .to_string(),
                                    )))
                                    .await;
                            }
                            Err(_) => {
                                let _ = store.mark_gen_failed(&gift_id);
                                let _ = tx
                                    .send(Ok(Event::default().data(
                                        "{\"status\":\"failed\"}",
                                    )))
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
        });

        Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
    }
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

    // Synced lyrics for the scrolling viewer. Prefer a provider-supplied LRC;
    // otherwise align our own from the lyrics + the track duration the provider
    // reports in diagnostic_metadata. Best-effort — never fail generation over it.
    let duration = result
        .diagnostic_metadata
        .get("duration_secs")
        .and_then(serde_json::Value::as_f64);
    let lrc = result.lrc.clone().or_else(|| {
        match (lyrics, duration) {
            (Some(text), Some(dur)) if dur > 0.0 => crate::lrc::generate_lrc(text, dur),
            _ => None,
        }
    });
    if let Some(lrc) = lrc {
        if let Err(e) = store.update_lrc(gift_id, &lrc, duration) {
            eprintln!("[music-gift] lrc store failed [{gift_id}]: {e}");
        }
    }

    Ok(url)
}
// ── Music Prompt Generation ──────────────────────────────────────────────────

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
) -> AppResult<EnrichedPrompt> {
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
            let tempo = parsed.get("tempo").and_then(str_or_empty).unwrap_or_default();
            let mood = parsed.get("mood").and_then(json_array).unwrap_or_default();
            let vocal_style = parsed.get("vocal_style").and_then(str_or_empty).unwrap_or_else(|| input.vocal.to_string());
            let instrumentation = parsed.get("instrumentation").and_then(str_or_empty).unwrap_or_default();
            let production = parsed.get("production").and_then(str_or_empty).unwrap_or_default();
            let exclude = parsed.get("exclude").and_then(str_or_empty).unwrap_or_default();
            let style_tags = parsed.get("style_tags").and_then(json_array).unwrap_or_default();

            return Ok(EnrichedPrompt {
                prompt,
                genre,
                tempo,
                mood,
                vocal_style,
                instrumentation,
                production,
                exclude,
                style_tags,
            });
        }
    }

    Ok(EnrichedPrompt::fallback(input.style))
}

fn json_array(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
}

fn str_or_empty(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

#[allow(dead_code)]
/// Extract the content of the first ```json ... ``` fenced code block.
fn extract_json_block(text: &str) -> Option<String> {
    let start_marker = "```json\n";
    let start = text.find(start_marker)? + start_marker.len();
    let rest = &text[start..];
    let end = rest.find("\n```")?;
    Some(rest[..end].to_string())
}
