//! Authentication: users, sessions, magic link tokens.
#![allow(dead_code, clippy::too_many_arguments)]

use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub provider: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub struct MagicToken {
    pub id: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub token: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Clone)]
pub struct AuthStore {
    conn: Arc<Mutex<Connection>>,
}

impl AuthStore {
    pub fn open(conn: Arc<Mutex<Connection>>) -> AppResult<Self> {
        {
            let c = conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS users (
                    id           TEXT PRIMARY KEY,
                    email        TEXT UNIQUE,
                    phone        TEXT UNIQUE,
                    display_name TEXT NOT NULL,
                    avatar_url   TEXT,
                    provider     TEXT NOT NULL,
                    provider_id  TEXT,
                    password_hash TEXT,
                    created_at   TEXT NOT NULL,
                    UNIQUE(provider, provider_id)
                );
                CREATE TABLE IF NOT EXISTS sessions (
                    id         TEXT PRIMARY KEY,
                    user_id    TEXT NOT NULL REFERENCES users(id),
                    token      TEXT NOT NULL UNIQUE,
                    created_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS magic_tokens (
                    id         TEXT PRIMARY KEY,
                    email      TEXT,
                    phone      TEXT,
                    token      TEXT NOT NULL UNIQUE,
                    used       INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                );
            ",)?;
            // creator_id might already exist
            let _ = c.execute("ALTER TABLE gifts ADD COLUMN creator_id TEXT REFERENCES users(id)", []);
        }
        Ok(Self { conn })
    }

    pub fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE email = ?1"
        )?;
        Ok(stmt.query_row(params![email], row_to_user).ok())
    }

    pub fn find_by_phone(&self, phone: &str) -> AppResult<Option<User>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE phone = ?1"
        )?;
        Ok(stmt.query_row(params![phone], row_to_user).ok())
    }

    pub fn find_by_provider(&self, provider: &str, provider_id: &str) -> AppResult<Option<User>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE provider = ?1 AND provider_id = ?2"
        )?;
        Ok(stmt.query_row(params![provider, provider_id], row_to_user).ok())
    }

    pub fn find_user(&self, id: &str) -> AppResult<Option<User>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE id = ?1"
        )?;
        Ok(stmt.query_row(params![id], row_to_user).ok())
    }

    pub fn create_user(
        &self, email: Option<&str>, phone: Option<&str>, display_name: &str,
        provider: &str, provider_id: Option<&str>,
    ) -> AppResult<User> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let now = unix_now();
        conn.execute(
            "INSERT INTO users (id, email, phone, display_name, avatar_url, provider, provider_id, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, email, phone, display_name, Option::<String>::None, provider, provider_id, now],
        )?;
        Ok(User { id, email: email.map(String::from), phone: phone.map(String::from), display_name: display_name.to_string(), avatar_url: None, provider: provider.to_string(), created_at: now })
    }

    pub fn create_user_with_password(&self, email: &str, password: &str, display_name: &str) -> AppResult<User> {
        let hash = hash_password(password);
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let now = unix_now();
        conn.execute(
            "INSERT INTO users (id, email, phone, display_name, avatar_url, provider, password_hash, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, email, Option::<String>::None, display_name, Option::<String>::None, "email", hash, now],
        )?;
        Ok(User { id, email: Some(email.to_string()), phone: None, display_name: display_name.to_string(), avatar_url: None, provider: "email".to_string(), created_at: now })
    }

    pub fn verify_password(&self, email: &str, password: &str) -> AppResult<Option<User>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT id, email, phone, display_name, avatar_url, provider, created_at, password_hash FROM users WHERE email = ?1")?;
        let result = stmt.query_row(params![email], |row| {
            let hash: Option<String> = row.get(7)?;
            Ok((row_to_user(row)?, hash))
        });
        match result {
            Ok((user, Some(hash))) if verify_password_hash(password, &hash) => Ok(Some(user)),
            _ => Ok(None),
        }
    }

    pub fn find_or_create_by_email(&self, email: &str, provider: &str) -> AppResult<User> {
        if let Some(user) = self.find_by_email(email)? { return Ok(user); }
        let name = email.split('@').next().unwrap_or(email);
        self.create_user(Some(email), None, name, provider, None)
    }

    // ── Sessions ────────────────────────────────────

    pub fn create_session(&self, user_id: &str) -> AppResult<Session> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().simple().to_string();
        let now = unix_now();
        let expires = unix_after(7 * 86400);
        conn.execute(
            "INSERT INTO sessions (id, user_id, token, created_at, expires_at) VALUES (?1,?2,?3,?4,?5)",
            params![id, user_id, token, now, expires],
        )?;
        Ok(Session { id, user_id: user_id.to_string(), token, created_at: now, expires_at: expires })
    }

    pub fn validate_session(&self, token: &str) -> AppResult<Option<User>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT u.id, u.email, u.phone, u.display_name, u.avatar_url, u.provider, u.created_at \
             FROM sessions s JOIN users u ON s.user_id = u.id \
             WHERE s.token = ?1 AND s.expires_at > ?2"
        )?;
        let now = unix_now();
        Ok(stmt.query_row(params![token, now], row_to_user).ok())
    }

    pub fn revoke_session(&self, token: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }

    // ── Magic tokens ────────────────────────────────

    pub fn create_magic_token(&self, email: Option<&str>, phone: Option<&str>) -> AppResult<MagicToken> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().simple().to_string();
        let now = unix_now();
        let expires = unix_after(600);
        conn.execute(
            "INSERT INTO magic_tokens (id, email, phone, token, used, created_at, expires_at) VALUES (?1,?2,?3,?4,0,?5,?6)",
            params![id, email, phone, token, now, expires],
        )?;
        Ok(MagicToken { id, email: email.map(String::from), phone: phone.map(String::from), token, created_at: now, expires_at: expires })
    }

    pub fn redeem_magic_token(&self, token: &str) -> AppResult<Option<MagicToken>> {
        let conn = self.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, token, used, created_at, expires_at FROM magic_tokens \
             WHERE token = ?1 AND used = 0 AND expires_at > ?2"
        )?;
        let now = unix_now();
        let mt = stmt.query_row(params![token, now], |row| {
            Ok(MagicToken {
                id: row.get(0)?, email: row.get(1)?, phone: row.get(2)?,
                token: row.get(3)?, created_at: row.get(5)?, expires_at: row.get(6)?,
            })
        }).ok();
        if mt.is_some() {
            conn.execute("UPDATE magic_tokens SET used = 1 WHERE token = ?1", params![token])?;
        }
        Ok(mt)
    }
}

// ── Email sending ────────────────────────────────────────

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub async fn send_magic_link_email(to: &str, token: &str, base_url: &str) -> Result<(), String> {
    let smtp_host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.resend.com".into());
    let smtp_user = std::env::var("SMTP_USER").unwrap_or_else(|_| "resend".into());
    let smtp_pass = std::env::var("SMTP_PASS").unwrap_or_default();
    let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| "Moment <noreply@moment.app>".into());

    let link = format!("{base_url}/api/auth/verify?token={token}");

    let email = Message::builder()
        .from(from.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .to(to.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject("Your Moment login link")
        .header(ContentType::TEXT_HTML)
        .body(format!(
            r#"<p>Tap the link below to sign in to Moment:</p>
            <p><a href="{link}">{link}</a></p>
            <p>This link expires in 10 minutes.</p>"#,
        ))
        .map_err(|e| e.to_string())?;

    let creds = Credentials::new(smtp_user, smtp_pass);
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_host)
        .map_err(|e| e.to_string())?
        .credentials(creds)
        .build();

    mailer.send(email).await.map_err(|e| e.to_string())?;
    Ok(())
}

// ── Route handlers ───────────────────────────────────────

use axum::{extract::{Query, State}, http::StatusCode, response::{IntoResponse, Redirect}, Json};
use axum_extra::extract::cookie::{Cookie, CookieJar};

#[derive(Deserialize)]
pub struct SendLinkRequest {
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: User,
    pub token: String,
}

pub async fn handle_send_link(
    State(state): State<crate::state::AppState>,
    Json(req): Json<SendLinkRequest>,
) -> impl IntoResponse {
    if let Some(email) = &req.email {
        if email.is_empty() || !email.contains('@') {
            return (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({"error":"INVALID_EMAIL"})));
        }
        let mt = match state.auth_store.create_magic_token(Some(email), None) {
            Ok(t) => t,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"INTERNAL"}))),
        };

        let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());
        if let Err(e) = send_magic_link_email(email, &mt.token, &base_url).await {
            eprintln!("[auth] failed to send email: {e}");
        }

        return (StatusCode::OK, Json(serde_json::json!({"ok":true})));
    }

    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"EMAIL_OR_PHONE_REQUIRED"})))
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

pub async fn handle_verify(
    State(state): State<crate::state::AppState>,
    Query(q): Query<VerifyQuery>,
    jar: CookieJar,
) -> impl IntoResponse {
    let mt = match state.auth_store.redeem_magic_token(&q.token) {
        Ok(Some(t)) => t,
        _ => return (StatusCode::NOT_FOUND, "Invalid or expired link").into_response(),
    };

    let provider = if mt.email.is_some() { "email" } else { "phone" };
    let user = match state.auth_store.find_or_create_by_email(
        mt.email.as_deref().unwrap_or(""),
        provider,
    ) {
        Ok(u) => u,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response(),
    };

    let session = match state.auth_store.create_session(&user.id) {
        Ok(s) => s,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response(),
    };

    let cookie = Cookie::build(("session_token", session.token))
        .path("/")
        .http_only(true)
        .secure(false) // TODO: true in production
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    (jar.add(cookie), Redirect::to(&format!("{base_url}/?auth_done=1"))).into_response()
}

pub async fn handle_me(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    let token = jar.get("session_token").map(|c| c.value().to_string());
    match token {
        Some(t) => match state.auth_store.validate_session(&t) {
            Ok(Some(user)) => (StatusCode::OK, Json(user)).into_response(),
            _ => (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"NOT_AUTHENTICATED"}))).into_response(),
        },
        None => (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"NOT_AUTHENTICATED"}))).into_response(),
    }
}

pub async fn handle_logout(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if let Some(token) = jar.get("session_token") {
        let _ = state.auth_store.revoke_session(token.value());
    }
    let cookie = Cookie::build(("session_token", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();
    (jar.add(cookie), Json(serde_json::json!({"ok":true})))
}


// ── Google OAuth ─────────────────────────────────────────

use serde_json::Value as JsonValue;

#[derive(Deserialize)]
pub struct GoogleCallback {
    pub code: String,
    pub state: String,
}

pub async fn handle_google_login(
    State(_state): State<crate::state::AppState>,
) -> impl IntoResponse {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/auth/oauth/google/cb".into());

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state=google",
        client_id, redirect_uri
    );
    Redirect::to(&url)
}

pub async fn handle_google_callback(
    State(state): State<crate::state::AppState>,
    Query(q): Query<GoogleCallback>,
    jar: CookieJar,
) -> impl IntoResponse {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/auth/oauth/google/cb".into());

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let token_resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", q.code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let token_data = match token_resp {
        Ok(r) => r.json::<JsonValue>().await.unwrap_or_default(),
        Err(_) => return (StatusCode::UNAUTHORIZED, "OAuth failed").into_response(),
    };

    let access_token = token_data.get("access_token").and_then(|v| v.as_str()).unwrap_or("");
    if access_token.is_empty() {
        return (StatusCode::UNAUTHORIZED, "OAuth failed").into_response();
    }

    // Get user info
    let user_data = match client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
    {
        Ok(r) => r.json::<JsonValue>().await.unwrap_or_default(),
        Err(_) => return (StatusCode::UNAUTHORIZED, "OAuth failed").into_response(),
    };

    let google_id = user_data.get("sub").and_then(|v| v.as_str()).unwrap_or("");
    let email = user_data.get("email").and_then(|v| v.as_str());
    let name = user_data.get("name").and_then(|v| v.as_str()).unwrap_or("User");

    let user = match state.auth_store.find_by_provider("google", google_id) {
        Ok(Some(u)) => u,
        _ => state.auth_store.create_user(email, None, name, "google", Some(google_id)).unwrap(),
    };

    let session = state.auth_store.create_session(&user.id).unwrap();

    let cookie = Cookie::build(("session_token", session.token))
        .path("/")
        .http_only(true)
        .secure(false)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    (jar.add(cookie), Redirect::to(&format!("{base_url}/?auth_done=1"))).into_response()
}
fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?, email: row.get(1)?, phone: row.get(2)?,
        display_name: row.get(3)?, avatar_url: row.get(4)?,
        provider: row.get(5)?, created_at: row.get(6)?,
    })
}

fn unix_now() -> String { chrono::Utc::now().timestamp().to_string() }
fn unix_after(secs: i64) -> String { (chrono::Utc::now().timestamp() + secs).to_string() }

// ── Password hashing ─────────────────────────────────

fn hash_password(pw: &str) -> String {
    use argon2::{password_hash::{PasswordHasher, SaltString}, Argon2};
    use rand_core::OsRng;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(pw.as_bytes(), &salt).unwrap().to_string()
}

fn verify_password_hash(pw: &str, hash: &str) -> bool {
    use argon2::{password_hash::PasswordVerifier, Argon2};
    let parsed = argon2::PasswordHash::new(hash).ok();
    parsed.map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok()).unwrap_or(false)
}

// ── Register / Login ────────────────────────────────────

use serde_json::json;

#[derive(Deserialize)]
pub struct RegisterRequest { pub email: String, pub password: String, pub display_name: String }
#[derive(Deserialize)]
pub struct LoginRequest { pub email: String, pub password: String }

pub async fn handle_register(
    State(state): State<crate::state::AppState>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    if body.email.is_empty() || !body.email.contains('@') {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error":"INVALID_EMAIL"}))).into_response();
    }
    if body.password.len() < 8 {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error":"WEAK_PASSWORD"}))).into_response();
    }
    match state.auth_store.create_user_with_password(&body.email, &body.password, &body.display_name) {
        Ok(user) => {
            let session = state.auth_store.create_session(&user.id).unwrap();
            (StatusCode::OK, Json(AuthResponse { user, token: session.token })).into_response()
        }
        Err(_) => (StatusCode::CONFLICT, Json(json!({"error":"EMAIL_EXISTS"}))).into_response(),
    }
}

pub async fn handle_login(
    State(state): State<crate::state::AppState>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    match state.auth_store.verify_password(&body.email, &body.password) {
        Ok(Some(user)) => {
            let session = state.auth_store.create_session(&user.id).unwrap();
            (StatusCode::OK, Json(AuthResponse { user, token: session.token })).into_response()
        }
        _ => (StatusCode::UNAUTHORIZED, Json(json!({"error":"INVALID_CREDENTIALS"}))).into_response(),
    }
}
