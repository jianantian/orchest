//! Axum route handlers for all API endpoints.
use std::convert::Infallible;

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::agent::{
    build_messages, build_photo_blocks, build_system_message, parse_lyrics,
    run_chat_agent, ChatRequest, SseEvent,
};
use crate::error::{AppError, AppResult};
use crate::gift::{Gift, GiftMeta};
use crate::state::AppState;
use crate::tools::music_gen::EnrichedPrompt;
/// Build the full router with all API routes + static file serving.
pub fn build_router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/chat", post(chat_handler))
        .route("/gift", post(create_gift))
        .route("/gift/{id}", get(get_gift).delete(delete_gift))
        .route("/gift/{id}/publish", post(set_gift_published))
        .route("/playlist", get(list_playlist))
        .route("/generate/{id}", post(generate_music))
        .route("/generate/{id}/status", get(generate_status))
        .route("/generate/{id}/stream", get(generate_stream))
        .route("/gift/{id}/lrc", get(get_gift_lrc))
        .route("/gift/{id}/like", post(like_gift))
        .route("/polish-music-prompt", post(polish_music_prompt))
        .route("/countdown-section/{id}", get(get_countdown_section))
        .route("/photos", post(upload_photos))
        // Auth (module wired via crate::auth)
        .route("/auth/me", get(crate::auth::handle_me))
        .route("/auth/logout", post(crate::auth::handle_logout))
        .route("/auth/register", post(crate::auth::handle_register))
        .route("/auth/login", post(crate::auth::handle_login))
        .route("/auth/send-link", post(crate::auth::handle_send_link))
        .route("/auth/verify", get(crate::auth::handle_verify))
        .route("/auth/oauth/google", get(crate::auth::handle_google_login))
        .route("/auth/oauth/google/cb", get(crate::auth::handle_google_callback))
        .with_state(state.clone());

    let mut router = Router::new().nest("/api", api);

    // Serve generated audio files from data/audio/
    let audio_dir = state.data_dir.join("audio");
    if std::fs::create_dir_all(&audio_dir).is_ok() {
        router = router.nest_service("/audio", tower_http::services::ServeDir::new(audio_dir));
    }
    // SPA: ServeDir handles static files. Two explicit routes serve index.html
    // for the client-side routes that react-router manages.
    if let Some(dir) = static_dir {
        let index_path = dir.join("index.html");
        let index_html = std::fs::read_to_string(&index_path).unwrap_or_default();

        let html = std::sync::Arc::new(index_html);
        let h1 = html.clone();
        let h2 = html.clone();
        let h3 = html.clone();

        async fn spa_fallback(State(html): State<std::sync::Arc<String>>) -> axum::response::Html<String> {
            axum::response::Html((*html).clone())
        }

        router = router
            .route("/gift/{id}", get(spa_fallback).with_state(h1))
            .route("/playlist", get(spa_fallback).with_state(h2))
            .route("/", get(spa_fallback).with_state(h3))
            .fallback_service(tower_http::services::ServeDir::new(dir));
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

    let (tx, rx) = mpsc::channel::<SseEvent>(64);

    let review_model = state.chat_model.clone();
    tokio::spawn(async move {
        let skills_dir = state.skills_dir.to_string_lossy().to_string();
        let result = run_chat_agent(state.chat_model.clone(), messages, tx.clone(), Some(&skills_dir)).await;
        match result {
            Ok(full_text) => {
                let has_lyrics = full_text.contains("<<<LYRICS>>>");
                // Only run review pass when the agent actually generated lyrics.
                // Skip it for follow-up questions — the reviewer gets confused
                // by conversational text.
                let reviewed = if has_lyrics {
                    // The review pass is a second full LLM call (tens of
                    // seconds). Tell the client before going quiet, or the UI
                    // sits frozen with a disabled input and no explanation.
                    let _ = tx.send(SseEvent::Reviewing).await;
                    crate::agent::run_review_pass(review_model, &full_text).await
                } else {
                    full_text.clone()
                };
                let parsed = parse_lyrics(&reviewed);
                let review = if has_lyrics {
                    crate::agent::extract_review_summary(&reviewed)
                } else {
                    None
                };
                let done = SseEvent::Done {
                    has_lyrics: parsed.has_lyrics,
                    lyrics: parsed.lyrics,
                    style: parsed.style,
                    title: parsed.title,
                    vocal: parsed.vocal,
                    review,
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
    jar: CookieJar,
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
    let meta_view = GiftMeta::from_value(&meta);
    let birthday: Option<String> = meta_view.birthday.clone();
    let gift_name: String = meta_view.name_or_default().to_string();
    let gift_scenario: String = meta_view.scenario.clone().unwrap_or_default();
    let gift_lyrics: String = req.lyrics.clone().unwrap_or_default();

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
        cover_url: None,
        photos: req.photos.into_iter().take(5).collect(),
        gen_handle: None,
        gen_status: None,
        countdown_status,
        lrc: None,
        duration_secs: None,
        creator_token: creator_token.clone(),
        // Private by default: a gift is personal until its creator chooses to
        // list it. It stays reachable by id, so sharing the link still works.
        published: false,
        likes: Vec::new(),
        created_at: now,
        published_at: None,
    };

    state.gift_store.create(&gift)?;

    // Best-effort ownership link: a valid session records the user on the
    // gift; anything else (no cookie, expired session, lookup failure) just
    // leaves creator_id NULL. The creator_token flow is unaffected and stays
    // the only mutation check.
    if let Some(token) = jar.get("session_token") {
        match state.auth_store.validate_session(token.value()) {
            Ok(Some(user)) => {
                if let Err(e) = state.gift_store.update_creator_id(&id, &user.id) {
                    eprintln!("[music-gift] creator_id link failed [{id}]: {e}");
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("[music-gift] session validation failed [{id}]: {e}"),
        }
    }

    if let (Some(bday), Some(tool)) = (birthday, state.countdown_tool.clone()) {
        let params = crate::tools::countdown::CountdownParams {
            name: gift_name.clone(),
            birthday: bday,
            scenario: gift_scenario.clone(),
            lyric_snippet: gift_lyrics.clone(),
        };
        let sink = crate::tools::countdown::CountdownSink {
            store: state.gift_store.clone(),
            data_dir: state.data_dir.clone(),
            gift_id: id.clone(),
        };
        let cd_id = id.clone();
        tokio::spawn(async move {
            // Discarding this Result made every countdown failure invisible:
            // the gift just sat on countdown_status="pending" forever, and the
            // frontend polled a file that was never going to appear.
            if let Err(e) = crate::tools::countdown::run_countdown(tool, &params, &sink).await {
                eprintln!("[music-gift] countdown failed [{cd_id}]: {e}");
                let _ = sink.store.update_countdown_status(&cd_id, "failed");
            }
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
// Creator-only operations
//
// `creator_token` is the only ownership proof available: it is minted at
// creation and returned once, to that client. Anyone holding a gift id can
// view it (that is how sharing works), so mutating routes must check the token.
// ---------------------------------------------------------------------------

const CREATOR_TOKEN_HEADER: &str = "x-creator-token";

fn verify_creator(gift: &Gift, headers: &HeaderMap) -> AppResult<()> {
    let presented = headers
        .get(CREATOR_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if presented.is_empty() || presented != gift.creator_token {
        return Err(AppError::Forbidden(
            "only the creator can modify this gift".to_string(),
        ));
    }
    Ok(())
}

// DELETE /api/gift/:id - permanently remove a gift and its audio file
pub async fn delete_gift(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;
    verify_creator(&gift, &headers)?;

    // Best-effort file cleanup: the row is the source of truth, so a stray
    // file must not fail the delete.
    if let Some(url) = gift.audio_url.as_deref() {
        if let Some(file) = url.strip_prefix("/audio/") {
            if !file.is_empty() && !file.contains("..") && !file.contains('/') {
                let path = state.data_dir.join("audio").join(file);
                if let Err(e) = std::fs::remove_file(&path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("[music-gift] delete [{id}]: audio cleanup: {e}");
                    }
                }
            }
        }
    }
    let cd = state.data_dir.join("countdown").join(format!("{id}.html"));
    let _ = std::fs::remove_file(cd);

    state.gift_store.delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SetPublishedRequest {
    pub published: bool,
}

// POST /api/gift/:id/publish - list or unlist on the public playlist
pub async fn set_gift_published(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SetPublishedRequest>,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;
    verify_creator(&gift, &headers)?;
    // Must match create_gift's format: published_at is string-sorted by
    // `ORDER BY published_at DESC`, so a mixed format corrupts the ordering.
    let now = unix_now();
    state.gift_store.set_published(&id, req.published, &now)?;
    Ok(Json(json!({ "ok": true, "published": req.published })))
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
        .map(|g| {
            let m = g.meta();
            PlaylistItem {
                id: g.id,
                title: m.title.unwrap_or_default(),
                name: m.name.unwrap_or_default(),
                relationship: m.relationship.unwrap_or_default(),
                // Display falls back to empty, not the generation default.
                style: m.style.unwrap_or_default(),
                lang: m.lang.unwrap_or_else(|| GiftMeta::DEFAULT_LANG.to_string()),
                audio_url: g.audio_url,
                likes: g.likes.len(),
                published_at: g.published_at,
            }
        })
        .collect();
    Ok(Json(PlaylistResponse { items }))
}

// ---------------------------------------------------------------------------
// POST /api/generate/:id - submit music generation
// ---------------------------------------------------------------------------

pub async fn generate_music(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let resp = crate::tools::music_gen::generate(
        &state.gen_task,
        &state.gift_store,
        &state.music_prompt_model,
        &state.music_provider,
        &id,
    )
    .await?;
    Ok((StatusCode::OK, Json(resp)))
}
pub async fn generate_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    crate::tools::music_gen::stream(state.gen_task.clone(), state.gift_store.clone(), id)
}

// ── POST /api/polish-music-prompt — generate optimized music prompt ──────

#[derive(Debug, Deserialize)]
pub struct PolishPromptRequest {
    pub lyrics: String,
    pub style: String,
    /// Optional per-request override; falls back to the startup-resolved
    /// provider on `AppState`.
    #[serde(default)]
    pub provider: Option<String>,
    pub title: Option<String>,
    pub vocal: Option<String>,
    pub scene: Option<String>,
    pub name: Option<String>,
    pub relationship: Option<String>,
    pub lang: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PolishPromptResponse {
    pub prompt: String,
}

pub async fn polish_music_prompt(
    State(state): State<AppState>,
    Json(req): Json<PolishPromptRequest>,
) -> AppResult<impl IntoResponse> {
    let enriched = crate::tools::music_gen::generate_music_prompt(
        state.music_prompt_model.clone(),
        &crate::tools::music_gen::MusicPromptInput {
            provider: req.provider.as_deref().unwrap_or(&state.music_provider),
            lyrics: &req.lyrics,
            style: &req.style,
            title: req.title.as_deref().unwrap_or(""),
            vocal: req.vocal.as_deref().unwrap_or(GiftMeta::DEFAULT_VOCAL),
            scene: req.scene.as_deref().unwrap_or(""),
            name: req.name.as_deref().unwrap_or(""),
            relationship: req.relationship.as_deref(),
            lang: req.lang.as_deref().unwrap_or(GiftMeta::DEFAULT_LANG),
        },
    ).await.unwrap_or_else(|_| EnrichedPrompt::fallback(&req.style));
    Ok(Json(PolishPromptResponse { prompt: enriched.prompt }))
}
// ── GET /api/generate/:id/status — legacy poll ────────────────────────────

pub async fn generate_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let resp = crate::tools::music_gen::poll(&state.gen_task, &state.gift_store, &id).await?;
    Ok(Json(resp))
}
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

// ── GET /api/gift/:id/lrc — return LRC text ────────────────────────────────

pub async fn get_gift_lrc(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let gift = state.gift_store.get(&id)?;
    match gift.lrc {
        Some(lrc) => Ok((StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], lrc)),
        None => Err(AppError::NotFound("LRC not available".to_string())),
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
