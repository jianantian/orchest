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
    /// Same logic as the original `generate_music` handler.
    pub async fn submit(&self, store: &GiftStore, gift_id: &str, prompt: &str) -> AppResult<GenerateResponse> {
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
        });

        let gen_req = GenRequest { prompt: prompt.to_string(), params };
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

/// Fetch completed generation assets, extract the first audio URL, and update
/// the store.
async fn handle_done(
    gen_task: &dyn GenTask,
    store: &GiftStore,
    gift_id: &str,
    _lyrics: Option<&str>,
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

    Ok(url)
}
// ── Music Prompt Generation ──────────────────────────────────────────────────

#[allow(dead_code, clippy::too_many_arguments)]
/// Generate an optimized music-generation prompt using a provider-specific skill.
///
/// Loads the skill template for `provider` ("suno", "mureka", "minimax"),
/// substitutes the song info, calls the LLM, and returns the generated prompt
/// string from the JSON response.
pub async fn generate_music_prompt(
    chat_model: Arc<dyn ChatModel>,
    provider: &str,
    lyrics: &str,
    style: &str,
    title: &str,
    vocal: &str,
    scene: &str,
    name: &str,
    relationship: Option<&str>,
) -> AppResult<String> {
    let skill_template = MUSIC_PROMPT_SKILLS.get(provider).ok_or_else(|| {
        AppError::BadRequest(format!("unknown music provider: {provider}"))
    })?;

    let relationship = relationship.unwrap_or("friend");

    let user_message = skill_template
        .replace("{lyrics}", lyrics)
        .replace("{style}", style)
        .replace("{title}", title)
        .replace("{vocal}", vocal)
        .replace("{scene}", scene)
        .replace("{name}", name)
        .replace("{relationship}", relationship);

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

    // Try the raw text as JSON first, then look for a code-fenced block.
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
                .unwrap_or(&text);
            return Ok(prompt.to_string());
        }
    }

    // Fallback: return the raw text trimmed.
    Ok(text.trim().to_string())
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
