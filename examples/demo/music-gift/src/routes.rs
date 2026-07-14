//! Axum route handlers for all API endpoints.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::time::{sleep, Duration};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use orchest_protocol::{ChatModel, ContentBlock, GenAsset, GenHandle, GenRequest, GenStatus, GenTask, Message, RequestOptions, Role};

use crate::agent::{
    build_messages, build_photo_blocks, build_system_message, drive_stream, parse_lyrics,
    start_stream, ChatRequest, SseEvent,
};
use crate::error::{AppError, AppResult};
use crate::gift::{Gift, GiftStore};

/// Shared application state injected into all handlers.
#[derive(Clone)]
pub struct AppState {
    pub chat_model: Arc<dyn ChatModel>,
    pub gen_task: Arc<dyn GenTask>,
    pub gift_store: GiftStore,
    pub data_dir: PathBuf,
}

/// Build the full router with all API routes + static file serving.
pub fn build_router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/chat", post(chat_handler))
        .route("/gift", post(create_gift))
        .route("/gift/{id}", get(get_gift))
        .route("/playlist", get(list_playlist))
        .route("/generate/{id}", post(generate_music))
        .route("/generate/{id}/status", get(generate_status))
        .route("/generate/{id}/stream", get(generate_stream))
        .route("/gift/{id}/like", post(like_gift))
        .route("/countdown-section/{id}", get(get_countdown_section))
        .route("/photos", post(upload_photos))
        .with_state(state.clone());

    let mut router = Router::new().nest("/api", api);

    // Serve generated audio files from data/audio/
    let audio_dir = state.data_dir.join("audio");
    if std::fs::create_dir_all(&audio_dir).is_ok() {
        router = router.nest_service("/audio", tower_http::services::ServeDir::new(audio_dir));
    }

    // Serve static frontend files as fallback for non-API routes.
    // TODO: SPA routing - /playlist and /gift/:id currently return 404.
    // Fix with a catch-all that serves index.html for unmatched GET requests.
    if let Some(dir) = static_dir {
        router = router.fallback_service(tower_http::services::ServeDir::new(dir));
    }

    router
}

// ---------------------------------------------------------------------------
// POST /api/chat - SSE streaming chat
// ---------------------------------------------------------------------------

pub async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> AppResult<impl IntoResponse> {
    if req.messages.is_empty() {
        return Err(AppError::BadRequest("messages is required".to_string()));
    }

    let photo_blocks = build_photo_blocks(&req.photos, &state.data_dir.to_string_lossy());
    let system_msg = build_system_message(&req.meta, photo_blocks.len());
    let messages = build_messages(system_msg, &req.messages, &photo_blocks);

    let stream = start_stream(state.chat_model.clone(), messages);

    // Channel of SseEvent items; the spawned task pushes, the SSE stream pulls.
    let (tx, rx) = mpsc::channel::<SseEvent>(64);

    tokio::spawn(async move {
        let result = drive_stream(stream, tx.clone()).await;
        match result {
            Ok(full_text) => {
                let parsed = parse_lyrics(&full_text);
                let done = SseEvent::Done {
                    has_lyrics: parsed.has_lyrics,
                    lyrics: parsed.lyrics,
                    style: parsed.style,
                    title: parsed.title,
                    vocal: parsed.vocal,
                };
                let _ = tx.send(done).await;
            }
            Err(e) => {
                let _ = tx
                    .send(SseEvent::Error {
                        error: e.to_string(),
                    })
                    .await;
            }
        }
    });

    // Convert SseEvent -> axum Event for the SSE response
    let sse_stream = ReceiverStream::new(rx).map(|event| {
        let json = serde_json::to_string(&event).unwrap_or_default();
        Ok::<Event, std::convert::Infallible>(Event::default().data(json))
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// POST /api/gift - create a new gift
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateGiftRequest {
    pub lyrics: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub meta: Value,
    #[serde(default)]
    pub photos: Vec<String>,
    #[serde(default)]
    pub style: Option<String>,
}

fn default_kind() -> String {
    "song".to_string()
}

#[derive(Debug, Serialize)]
pub struct CreateGiftResponse {
    pub id: String,
    pub creator_token: String,
}

pub async fn create_gift(
    State(state): State<AppState>,
    Json(req): Json<CreateGiftRequest>,
) -> AppResult<impl IntoResponse> {
    let id = Uuid::new_v4().simple().to_string()[..12].to_string();
    let creator_token = Uuid::new_v4().to_string();
    let now = unix_now();

    // Merge style into meta
    let mut meta = req.meta;
    if let Some(style) = req.style {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("style".to_string(), Value::String(style));
        } else {
            meta = json!({ "style": style });
        }
    }

    // Check for birthday info before meta is moved into the gift.
    let birthday: Option<String> = meta
        .get("birthday")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let gift_name: String = meta
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Someone")
        .to_string();

    let countdown_status = if birthday.is_some() {
        Some("pending".to_string())
    } else {
        None
    };

    let gift = Gift {
        id: id.clone(),
        kind: req.kind,
        lyrics: req.lyrics,
        meta,
        audio_url: None,
        photos: req.photos.into_iter().take(5).collect(),
        gen_handle: None,
        gen_status: None,
        countdown_status,
        creator_token: creator_token.clone(),
        published: true,
        likes: Vec::new(),
        created_at: now.clone(),
        published_at: Some(now),
    };

    state.gift_store.create(&gift)?;

    // Spawn background countdown generation if birthday is present.
    if let Some(bday) = birthday {
        let model = state.chat_model.clone();
        let store = state.gift_store.clone();
        let data_dir = state.data_dir.clone();
        let gift_id = id.clone();
        tokio::spawn(async move {
            generate_countdown(&model, &store, &data_dir, &gift_id, &gift_name, &bday).await;
        });
    }
    Ok((
        StatusCode::CREATED,
        Json(CreateGiftResponse { id, creator_token }),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/gift/:id - get a gift
// ---------------------------------------------------------------------------

pub async fn get_gift(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;
    Ok(Json(gift))
}

// ---------------------------------------------------------------------------
// GET /api/playlist - list published gifts
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PlaylistItem {
    pub id: String,
    pub title: String,
    pub name: String,
    pub relationship: String,
    pub style: String,
    pub lang: String,
    pub audio_url: Option<String>,
    pub likes: usize,
    pub published_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlaylistResponse {
    pub items: Vec<PlaylistItem>,
}

pub async fn list_playlist(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let gifts = state.gift_store.list_published()?;
    let items: Vec<PlaylistItem> = gifts
        .into_iter()
        .filter(|g| g.audio_url.is_some())
        .map(|g| PlaylistItem {
            id: g.id,
            title: g
                .meta
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            name: g
                .meta
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            relationship: g
                .meta
                .get("relationship")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            style: g
                .meta
                .get("style")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            lang: g
                .meta
                .get("lang")
                .and_then(Value::as_str)
                .unwrap_or("en")
                .to_string(),
            audio_url: g.audio_url,
            likes: g.likes.len(),
            published_at: g.published_at,
        })
        .collect();
    Ok(Json(PlaylistResponse { items }))
}

// ---------------------------------------------------------------------------
// POST /api/generate/:id - submit music generation
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub id: String,
    pub status: String,
    pub handle: Option<String>,
}

pub async fn generate_music(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;

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
    let handle = state.gen_task.submit(gen_req).await?;
    let handle_json = serde_json::to_string(&handle)?;

    state.gift_store.update_gen(&id, &handle_json, "pending")?;

    Ok((
        StatusCode::OK,
        Json(GenerateResponse {
            id: id.clone(),
            status: "pending".to_string(),
            handle: Some(handle_json),
        }),
    ))
}

// ── GET /api/generate/:id/stream — SSE streaming status ──────────────────

pub async fn generate_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(8);

    tokio::spawn(async move {
        let handle = match load_gen_handle(&state, &id).await {
            Ok(h) => h,
            Err(_) => { let _ = tx.send(Ok(Event::default().data("{\"status\":\"error\"}"))).await; return; }
        };
        let _ = tx.send(Ok(Event::default().data("{\"status\":\"pending\"}"))).await;

        for _ in 0..48 { // poll up to 4 minutes
            sleep(Duration::from_secs(5)).await;
            match state.gen_task.poll(&handle).await {
                Ok(GenStatus::Done) => {
                    match state.gen_task.fetch(&handle).await {
                        Ok(result) => {
                            let url = result.assets.first().and_then(|a| match a {
                                GenAsset::Url { url, .. } => Some(url.clone()),
                                _ => None,
                            });
                            if let Some(ref u) = url { let _ = state.gift_store.update_audio(&id, u); }
                            let _ = tx.send(Ok(Event::default().data(json!({"status":"done","audio_url":url}).to_string()))).await;
                        }
                        Err(_) => { let _ = state.gift_store.mark_gen_failed(&id); let _ = tx.send(Ok(Event::default().data("{\"status\":\"failed\"}"))).await; }
                    }
                    return;
                }
                Ok(GenStatus::Failed) => { let _ = state.gift_store.mark_gen_failed(&id); let _ = tx.send(Ok(Event::default().data("{\"status\":\"failed\"}"))).await; return; }
                Ok(GenStatus::Running) => { let _ = tx.send(Ok(Event::default().data("{\"status\":\"running\"}"))).await; }
                _ => {} // pending or error, keep polling
            }
        }
        let _ = tx.send(Ok(Event::default().data("{\"status\":\"timeout\"}"))).await;
    });
    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

async fn load_gen_handle(state: &AppState, id: &str) -> AppResult<GenHandle> {
    let gift = state.gift_store.get(id)?;
    let json = gift.gen_handle.ok_or_else(|| AppError::BadRequest("not submitted".into()))?;
    serde_json::from_str(&json).map_err(|e| AppError::BadRequest(format!("bad handle: {e}")))
}

// ── GET /api/generate/:id/status — legacy poll ────────────────────────────

#[derive(Debug, Serialize)]
pub struct GenStatusResponse {
    pub id: String, pub status: String, pub audio_url: Option<String>,
}

pub async fn generate_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;
    if gift.gen_status.as_deref() == Some("done") {
        return Ok(Json(GenStatusResponse {
            id,
            status: "done".to_string(),
            audio_url: gift.audio_url,
        }));
    }

    // If no handle, generation hasn't been submitted
    let handle_json = match &gift.gen_handle {
        Some(h) => h.clone(),
        None => return Err(AppError::BadRequest("generation not submitted".to_string())),
    };

    let handle: GenHandle = serde_json::from_str(&handle_json)
        .map_err(|e| AppError::BadRequest(format!("invalid gen handle: {e}")))?;

    let status = match state.gen_task.poll(&handle).await {
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
        match state.gen_task.fetch(&handle).await {
            Ok(result) => {
                let audio_url = result.assets.first().and_then(|asset| match asset {
                    GenAsset::Url { url, .. } => Some(url.clone()),
                    GenAsset::Bytes { .. } => None,
                });
                if let Some(url) = &audio_url {
                    state.gift_store.update_audio(&id, url)?;
                }
                return Ok(Json(GenStatusResponse {
                    id,
                    status: "done".to_string(),
                    audio_url,
                }));
            }
            Err(e) => {
                state.gift_store.mark_gen_failed(&id)?;
                return Err(AppError::Gen(e.to_string()));
            }
        }
    }

    if status == GenStatus::Failed {
        state.gift_store.mark_gen_failed(&id)?;
    }

    Ok(Json(GenStatusResponse {
        id,
        status: status_str.to_string(),
        audio_url: None,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/gift/:id/like - like a gift
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LikeRequest {
    pub viewer_id: String,
}

#[derive(Debug, Serialize)]
pub struct LikeResponse {
    pub ok: bool,
    pub likes: usize,
    pub liked: bool,
}

pub async fn like_gift(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<LikeRequest>,
) -> AppResult<impl IntoResponse> {
    let likes = state.gift_store.like(&id, &req.viewer_id)?;
    Ok(Json(LikeResponse {
        ok: true,
        likes,
        liked: true,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/photos - upload base64 photos
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PhotoUploadRequest {
    pub photos: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PhotoUploadResponse {
    pub urls: Vec<String>,
}

pub async fn upload_photos(
    State(state): State<AppState>,
    Json(req): Json<PhotoUploadRequest>,
) -> AppResult<impl IntoResponse> {
    if req.photos.is_empty() {
        return Err(AppError::BadRequest("no photos".to_string()));
    }
    if req.photos.len() > 5 {
        return Err(AppError::BadRequest("max 5 photos".to_string()));
    }

    let photos_dir = state.data_dir.join("photos");
    std::fs::create_dir_all(&photos_dir)?;

    let mut urls = Vec::new();
    for data_url in &req.photos {
        let (media_type, data) = parse_data_url(data_url)?;
        if data.len() > 8 * 1024 * 1024 {
            return Err(AppError::BadRequest("photo too large (>8MB)".to_string()));
        }
        let ext = match media_type.as_str() {
            "image/png" => "png",
            "image/webp" => "webp",
            _ => "jpg",
        };
        let filename = format!("{}.{}", Uuid::new_v4(), ext);
        let file_path = photos_dir.join(&filename);
        std::fs::write(&file_path, &data)?;
        urls.push(format!("/photos/{filename}"));
    }

    Ok(Json(PhotoUploadResponse { urls }))
}

/// Parse a `data:image/...;base64,...` URL into (media_type, raw_bytes).
fn parse_data_url(data_url: &str) -> AppResult<(String, Vec<u8>)> {
    // Format: data:image/<format>;base64,<data>
    let after_prefix = data_url
        .strip_prefix("data:image/")
        .ok_or_else(|| AppError::BadRequest("invalid data URL".to_string()))?;
    let (format, after_format) = after_prefix
        .split_once(';')
        .ok_or_else(|| AppError::BadRequest("invalid data URL".to_string()))?;
    if !["jpeg", "png", "webp"].contains(&format) {
        return Err(AppError::BadRequest(format!(
            "unsupported image format: {format}"
        )));
    }
    let b64 = after_format
        .strip_prefix("base64,")
        .ok_or_else(|| AppError::BadRequest("invalid data URL".to_string()))?;
    let media_type = match format {
        "jpeg" => "image/jpeg",
        "png" => "image/png",
        _ => "image/webp",
    };
    let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)?;
    Ok((media_type.to_string(), data))
}

/// Unix timestamp string for "now" (used as created_at / published_at).
fn unix_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}


// ---------------------------------------------------------------------------
// GET /api/countdown-section/:id - countdown HTML
// ---------------------------------------------------------------------------

pub async fn get_countdown_section(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;
    match gift.countdown_status.as_deref() {
        Some("ready") => {
            let path = state.data_dir.join("countdown").join(format!("{id}.html"));
            let html = tokio::fs::read_to_string(&path).await.map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    AppError::NotFound(format!("countdown HTML not found for gift {id}"))
                }
                _ => AppError::Io(e),
            })?;
            Ok((
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                html,
            )
                .into_response())
        }
        Some("pending") => Ok((
            StatusCode::ACCEPTED,
            Json(json!({"status": "pending"})),
        )
            .into_response()),
        _ => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"status": gift.countdown_status.as_deref().unwrap_or("unavailable")})),
        )
            .into_response()),
    }
}

/// Generate birthday countdown HTML via the LLM and save it to disk.
#[allow(clippy::too_many_arguments)]
async fn generate_countdown(
    model: &Arc<dyn ChatModel>,
    store: &GiftStore,
    data_dir: &std::path::Path,
    gift_id: &str,
    name: &str,
    birthday: &str,
) {
    let prompt = format!(
        "Generate a birthday countdown HTML page for {name} whose birthday is on {birthday}. \
         Create a beautiful, animated countdown timer showing days remaining until the next birthday. \
         Use CSS animations, a festive but elegant design, and make it a complete standalone HTML document. \
         Output ONLY the HTML code, starting with <!DOCTYPE html>. Do not wrap in code fences."
    );
    let messages = [Message {
        role: Role::User,
        content: vec![ContentBlock::Text(prompt)],
    }];
    let options = RequestOptions::default();

    let result = model.complete(&messages, &[], &options, None).await;
    match result {
        Ok(response) => {
            let html = response
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");

            // Strip markdown code fences if present.
            let html = html
                .trim()
                .strip_prefix("```html")
                .unwrap_or(&html)
                .strip_suffix("```")
                .unwrap_or(html.trim())
                .trim()
                .to_string();

            // Ensure the countdown directory exists.
            let dir = data_dir.join("countdown");
            if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                eprintln!("countdown: failed to create dir {}: {e}", dir.display());
                let _ = store.update_countdown_status(gift_id, "failed");
                return;
            }

            let path = dir.join(format!("{gift_id}.html"));
            if let Err(e) = tokio::fs::write(&path, &html).await {
                eprintln!("countdown: failed to write {}: {e}", path.display());
                let _ = store.update_countdown_status(gift_id, "failed");
                return;
            }

            let _ = store.update_countdown_status(gift_id, "ready");
        }
        Err(e) => {
            eprintln!("countdown: LLM error for gift {gift_id}: {e}");
            let _ = store.update_countdown_status(gift_id, "failed");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_data_url_jpeg() {
        let (mime, data) = parse_data_url("data:image/jpeg;base64,dGVzdA==").unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(data, b"test");
    }

    #[test]
    fn parse_data_url_png() {
        let (mime, data) = parse_data_url("data:image/png;base64,cG5n").unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(data, b"png");
    }

    #[test]
    fn parse_data_url_webp() {
        let (mime, data) = parse_data_url("data:image/webp;base64,d2VicA==").unwrap();
        assert_eq!(mime, "image/webp");
        assert_eq!(data, b"webp");
    }

    #[test]
    fn parse_data_url_rejects_invalid_prefix() {
        assert!(parse_data_url("not-a-data-url").is_err());
        assert!(parse_data_url("data:text/plain;base64,abc").is_err());
    }

    #[test]
    fn parse_data_url_rejects_unsupported_format() {
        assert!(parse_data_url("data:image/gif;base64,abc").is_err());
    }

    #[test]
    fn unix_now_returns_non_empty() {
        let now = unix_now();
        assert!(!now.is_empty());
        assert!(now.parse::<u64>().unwrap() > 1700000000); // after 2023
    }
}
