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

use orchest_protocol::{GenAsset, GenHandle, GenRequest, GenStatus, GenTask};

use crate::error::{AppError, AppResult};
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
    pub async fn submit(&self, store: &GiftStore, gift_id: &str) -> AppResult<GenerateResponse> {
        let gift = store.get(gift_id)?;

        let style = gift
            .meta
            .get("style")
            .and_then(Value::as_str)
            .unwrap_or("healing and warm");
        let base_prompt = "high quality music production";
        let lyrics = gift.lyrics.unwrap_or_default();
        let prompt = format!("{style}, {base_prompt}");

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

        let gen_req = GenRequest { prompt, params };
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

        // If no handle, generation hasn't been submitted
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
            // Transient network error: don't mark as failed, let the caller retry
            Err(e) => return Err(AppError::Gen(e.to_string())),
        };

        let status_str = match status {
            GenStatus::Pending => "pending",
            GenStatus::Running => "running",
            GenStatus::Done => "done",
            GenStatus::Failed => "failed",
        };

        if status == GenStatus::Done {
            // Fetch the result
            match self.gen_task.fetch(&handle).await {
                Ok(result) => {
                    let audio_url = result.assets.first().and_then(|asset| match asset {
                        GenAsset::Url { url, .. } => Some(url.clone()),
                        GenAsset::Bytes { .. } => None,
                    });
                    if let Some(url) = &audio_url {
                        store.update_audio(gift_id, url)?;
                    }
                    return Ok(GenStatusResponse {
                        id: gift_id.to_string(),
                        status: "done".to_string(),
                        audio_url,
                    });
                }
                Err(e) => {
                    store.mark_gen_failed(gift_id)?;
                    return Err(AppError::Gen(e.to_string()));
                }
            }
        }

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
    /// Same logic as the original `generate_stream` handler (poll loop + load
    /// handle + SSE wrapping).
    pub fn stream(
        gen_task: Arc<dyn GenTask>,
        store: GiftStore,
        gift_id: String,
    ) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(8);

        tokio::spawn(async move {
            let handle = match load_gen_handle(&store, &gift_id).await {
                Ok(h) => h,
                Err(_) => {
                    let _ = tx
                        .send(Ok(Event::default().data("{\"status\":\"error\"}")))
                        .await;
                    return;
                }
            };
            let _ = tx
                .send(Ok(Event::default().data("{\"status\":\"pending\"}")))
                .await;

            for _ in 0..48 {
                // poll up to 4 minutes
                sleep(Duration::from_secs(5)).await;
                match gen_task.poll(&handle).await {
                    Ok(GenStatus::Done) => {
                        match gen_task.fetch(&handle).await {
                            Ok(result) => {
                                let url = result.assets.first().and_then(|a| match a {
                                    GenAsset::Url { url, .. } => Some(url.clone()),
                                    _ => None,
                                });
                                if let Some(ref u) = url {
                                    let _ = store.update_audio(&gift_id, u);
                                }
                                let _ = tx
                                    .send(Ok(Event::default().data(
                                        json!({"status":"done","audio_url":url}).to_string(),
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

/// Load and deserialize a `GenHandle` from the gift store.
async fn load_gen_handle(store: &GiftStore, id: &str) -> AppResult<GenHandle> {
    let gift = store.get(id)?;
    let json = gift
        .gen_handle
        .ok_or_else(|| AppError::BadRequest("not submitted".into()))?;
    serde_json::from_str(&json).map_err(|e| AppError::BadRequest(format!("bad handle: {e}")))
}
