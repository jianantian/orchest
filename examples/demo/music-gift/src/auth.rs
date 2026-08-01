//! Authentication: users, sessions, password-reset tokens.
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
pub struct ResetToken {
    pub id: String,
    pub user_id: String,
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
                CREATE TABLE IF NOT EXISTS reset_tokens (
                    id         TEXT PRIMARY KEY,
                    user_id    TEXT NOT NULL REFERENCES users(id),
                    token      TEXT NOT NULL UNIQUE,
                    used       INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                );
            ",
            )?;
            // creator_id might already exist
            let _ = c.execute(
                "ALTER TABLE gifts ADD COLUMN creator_id TEXT REFERENCES users(id)",
                [],
            );
            // Superseded by reset_tokens (the old magic-link login flow was
            // removed); drop the stale table so its rows never linger.
            let _ = c.execute("DROP TABLE IF EXISTS magic_tokens", []);
        }
        Ok(Self { conn })
    }

    pub fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE email = ?1"
        )?;
        Ok(stmt.query_row(params![email], row_to_user).ok())
    }

    pub fn find_by_phone(&self, phone: &str) -> AppResult<Option<User>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE phone = ?1"
        )?;
        Ok(stmt.query_row(params![phone], row_to_user).ok())
    }

    pub fn find_by_provider(&self, provider: &str, provider_id: &str) -> AppResult<Option<User>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE provider = ?1 AND provider_id = ?2"
        )?;
        Ok(stmt
            .query_row(params![provider, provider_id], row_to_user)
            .ok())
    }

    pub fn find_user(&self, id: &str) -> AppResult<Option<User>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, email, phone, display_name, avatar_url, provider, created_at FROM users WHERE id = ?1"
        )?;
        Ok(stmt.query_row(params![id], row_to_user).ok())
    }

    pub fn create_user(
        &self,
        email: Option<&str>,
        phone: Option<&str>,
        display_name: &str,
        provider: &str,
        provider_id: Option<&str>,
    ) -> AppResult<User> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let now = unix_now();
        conn.execute(
            "INSERT INTO users (id, email, phone, display_name, avatar_url, provider, provider_id, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, email, phone, display_name, Option::<String>::None, provider, provider_id, now],
        )?;
        Ok(User {
            id,
            email: email.map(String::from),
            phone: phone.map(String::from),
            display_name: display_name.to_string(),
            avatar_url: None,
            provider: provider.to_string(),
            created_at: now,
        })
    }

    pub fn create_user_with_password(
        &self,
        email: &str,
        password: &str,
        display_name: &str,
    ) -> AppResult<User> {
        let hash = hash_password(password);
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let now = unix_now();
        conn.execute(
            "INSERT INTO users (id, email, phone, display_name, avatar_url, provider, password_hash, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, email, Option::<String>::None, display_name, Option::<String>::None, "email", hash, now],
        )?;
        Ok(User {
            id,
            email: Some(email.to_string()),
            phone: None,
            display_name: display_name.to_string(),
            avatar_url: None,
            provider: "email".to_string(),
            created_at: now,
        })
    }

    pub fn verify_password(&self, email: &str, password: &str) -> AppResult<Option<User>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
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

    /// Set or replace the account's password (signed-in user, session
    /// checked by the handler). Also used by the password-reset flow:
    /// `handle_reset` redeems the reset token and calls this.
    pub fn set_password(&self, user_id: &str, password: &str) -> AppResult<()> {
        let hash = hash_password(password);
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE users SET password_hash=?2 WHERE id=?1",
            params![user_id, hash],
        )? == 0
        {
            return Err(AppError::NotFound(format!("user {user_id} not found")));
        }
        Ok(())
    }

    /// Attach an OAuth provider identity to an existing account (email
    /// match). Lets a Google sign-in land on the same account as the
    /// password/magic one instead of dead-ending in EMAIL_EXISTS.
    pub fn link_provider(&self, user_id: &str, provider: &str, provider_id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE users SET provider=?2, provider_id=?3 WHERE id=?1",
            params![user_id, provider, provider_id],
        )?;
        Ok(())
    }

    // ── Sessions ────────────────────────────────────

    pub fn create_session(&self, user_id: &str) -> AppResult<Session> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().simple().to_string();
        let now = unix_now();
        let expires = unix_after(7 * 86400);
        conn.execute(
            "INSERT INTO sessions (id, user_id, token, created_at, expires_at) VALUES (?1,?2,?3,?4,?5)",
            params![id, user_id, token, now, expires],
        )?;
        Ok(Session {
            id,
            user_id: user_id.to_string(),
            token,
            created_at: now,
            expires_at: expires,
        })
    }

    pub fn validate_session(&self, token: &str) -> AppResult<Option<User>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT u.id, u.email, u.phone, u.display_name, u.avatar_url, u.provider, u.created_at \
             FROM sessions s JOIN users u ON s.user_id = u.id \
             WHERE s.token = ?1 AND s.expires_at > ?2"
        )?;
        let now = unix_now();
        Ok(stmt.query_row(params![token, now], row_to_user).ok())
    }

    pub fn revoke_session(&self, token: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }

    /// Drop every session for a user. Called after a password reset: a
    /// password change invalidates any session minted under the old one.
    pub fn revoke_user_sessions(&self, user_id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id])?;
        Ok(())
    }

    // ── Password-reset tokens ───────────────────────

    pub fn create_reset_token(&self, user_id: &str) -> AppResult<ResetToken> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().simple().to_string();
        let now = unix_now();
        let expires = unix_after(30 * 60);
        conn.execute(
            "INSERT INTO reset_tokens (id, user_id, token, used, created_at, expires_at) \
             VALUES (?1,?2,?3,0,?4,?5)",
            params![id, user_id, token, now, expires],
        )?;
        Ok(ResetToken {
            id,
            user_id: user_id.to_string(),
            token,
            created_at: now,
            expires_at: expires,
        })
    }

    /// Redeem a reset token: single-use, 30-minute lifetime. Returns the
    /// owning user id on success and marks the token used.
    pub fn redeem_reset_token(&self, token: &str) -> AppResult<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT user_id FROM reset_tokens \
             WHERE token = ?1 AND used = 0 AND expires_at > ?2",
        )?;
        let now = unix_now();
        let user_id = stmt
            .query_row(params![token, now], |row| row.get::<_, String>(0))
            .ok();
        if user_id.is_some() {
            conn.execute(
                "UPDATE reset_tokens SET used = 1 WHERE token = ?1",
                params![token],
            )?;
        }
        Ok(user_id)
    }
}

// ── Rate limiting ──────────────────────────────────────────

/// Demo-grade in-memory fixed-window limiter. Keys are email addresses
/// (login/register/forgot) — IP-based limiting would need ConnectInfo
/// plumbing and buys little on localhost. A determined attacker can rotate
/// emails; this stops casual spam and brute force.
#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<std::collections::HashMap<String, std::collections::VecDeque<i64>>>,
}

impl RateLimiter {
    /// Record a hit and report whether it is within `max` hits per
    /// `window_secs`.
    pub fn allow(&self, key: &str, max: usize, window_secs: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        let mut hits = match self.hits.lock() {
            Ok(h) => h,
            Err(poisoned) => poisoned.into_inner(),
        };
        let q = hits.entry(key.to_string()).or_default();
        while q.front().is_some_and(|t| now - *t > window_secs) {
            q.pop_front();
        }
        if q.len() >= max {
            return false;
        }
        q.push_back(now);
        true
    }
}

// ── Email sending ────────────────────────────────────────

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub async fn send_reset_email(to: &str, token: &str, base_url: &str) -> Result<(), String> {
    let smtp_host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.resend.com".into());
    let smtp_user = std::env::var("SMTP_USER").unwrap_or_else(|_| "resend".into());
    let smtp_pass = std::env::var("SMTP_PASS").unwrap_or_default();
    let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| "Moment <noreply@moment.app>".into());

    // The reset link opens the SPA's /reset-password page (token in query),
    // not a backend endpoint — the page posts to POST /api/auth/reset.
    let link = format!("{base_url}/reset-password?token={token}");

    let email = Message::builder()
        .from(
            from.parse()
                .map_err(|e: lettre::address::AddressError| e.to_string())?,
        )
        .to(to
            .parse()
            .map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject("Reset your Moment password")
        .header(ContentType::TEXT_HTML)
        .body(format!(
            r#"<p>Tap the link below to reset your Moment password:</p>
            <p><a href="{link}">{link}</a></p>
            <p>This link expires in 30 minutes. If you didn't ask for a
            password reset, you can ignore this email.</p>"#,
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

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: User,
    pub token: String,
}

#[derive(Deserialize)]
pub struct ForgotRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct ResetRequest {
    pub token: String,
    pub password: String,
}

/// POST /api/auth/forgot — email a password-reset link.
///
/// Answers identically whether or not the email exists (no account
/// enumeration); unknown emails simply skip token creation. Rate-limited
/// per email so a bored client cannot spam the mail relay (or the dev log).
pub async fn handle_forgot(
    State(state): State<crate::state::AppState>,
    Json(req): Json<ForgotRequest>,
) -> impl IntoResponse {
    if req.email.is_empty() || !req.email.contains('@') {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error":"INVALID_EMAIL"})),
        )
            .into_response();
    }
    if !state
        .rate_limiter
        .allow(&format!("forgot:{}", req.email.to_lowercase()), 5, 3600)
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error":"RATE_LIMITED"})),
        )
            .into_response();
    }

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    if let Ok(Some(user)) = state.auth_store.find_by_email(&req.email) {
        if let Ok(rt) = state.auth_store.create_reset_token(&user.id) {
            let smtp_pass = std::env::var("SMTP_PASS").unwrap_or_default();
            let link = format!("{base_url}/reset-password?token={}", rt.token);
            if smtp_pass.is_empty() {
                // No SMTP configured (local dev): surface the link in the
                // server log so the flow stays testable before SMTP exists.
                eprintln!("[auth] dev mode, reset link for {}: {link}", req.email);
            } else if let Err(e) = send_reset_email(&req.email, &rt.token, &base_url).await {
                eprintln!("[auth] failed to send reset email: {e}");
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({"ok":true}))).into_response()
}

/// POST /api/auth/reset — redeem a reset token and set a new password.
///
/// Single-use, 30-minute tokens. A successful reset invalidates every
/// session minted under the old password and signs the user in fresh.
pub async fn handle_reset(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
    Json(req): Json<ResetRequest>,
) -> impl IntoResponse {
    let user_id = match state.auth_store.redeem_reset_token(&req.token) {
        Ok(Some(id)) => id,
        _ => {
            return (
                StatusCode::GONE,
                Json(serde_json::json!({"error":"INVALID_TOKEN"})),
            )
                .into_response()
        }
    };
    if req.password.len() < 8 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error":"WEAK_PASSWORD"})),
        )
            .into_response();
    }
    if let Err(e) = state.auth_store.set_password(&user_id, &req.password) {
        eprintln!("[auth] reset set_password failed: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"INTERNAL"})),
        )
            .into_response();
    }
    // The password change invalidates every session minted under the old one.
    let _ = state.auth_store.revoke_user_sessions(&user_id);
    let session = match state.auth_store.create_session(&user_id) {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"INTERNAL"})),
            )
                .into_response()
        }
    };
    let cookie = session_cookie(session.token.clone());
    (jar.add(cookie), Json(serde_json::json!({"ok":true}))).into_response()
}

pub async fn handle_me(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    let token = jar.get("session_token").map(|c| c.value().to_string());
    match token {
        Some(t) => match state.auth_store.validate_session(&t) {
            Ok(Some(user)) => (StatusCode::OK, Json(user)).into_response(),
            _ => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error":"NOT_AUTHENTICATED"})),
            )
                .into_response(),
        },
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"NOT_AUTHENTICATED"})),
        )
            .into_response(),
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
    jar: CookieJar,
) -> impl IntoResponse {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
    // Fail loudly instead of redirecting to Google with an empty client_id
    // (which surfaces as the cryptic "Access blocked: Authorization Error"
    // page users have no way to act on).
    if client_id.is_empty() || client_secret.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "<h1>Google sign-in is not configured</h1>\
             <p>The server is missing <code>GOOGLE_CLIENT_ID</code> / <code>GOOGLE_CLIENT_SECRET</code>.</p>\
             <p>Create an OAuth client in <a href=\"https://console.cloud.google.com/apis/credentials\">Google Cloud Console</a>,\n\
             add <code>http://localhost:3000/api/auth/oauth/google/cb</code> to its\n\
             <em>Authorized redirect URIs</em>, then set the two variables in\n\
             <code>examples/demo/music-gift/.env</code> (see .env.example).</p>",
        )
            .into_response();
    }

    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/auth/oauth/google/cb".into());

    // Random state bound to this browser (login CSRF): the callback must
    // see a state this server issued, or an attacker could log a victim
    // into the attacker's account.
    let state = Uuid::new_v4().simple().to_string();
    let state_cookie = Cookie::build(("oauth_state", state.clone()))
        .path("/api/auth/oauth/google/cb")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::minutes(10))
        .build();

    // Query-escape the redirect URI so Google parses it as one parameter.
    let encoded_uri = redirect_uri.replace(':', "%3A").replace('/', "%2F");
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}",
        client_id, encoded_uri, state
    );
    (jar.add(state_cookie), Redirect::to(&url)).into_response()
}

pub async fn handle_google_callback(
    State(state): State<crate::state::AppState>,
    Query(q): Query<GoogleCallback>,
    jar: CookieJar,
) -> impl IntoResponse {
    // Reject callbacks whose state this server never issued (login CSRF).
    let issued = jar
        .get("oauth_state")
        .map(|c| c.value())
        .unwrap_or_default();
    if issued.is_empty() || issued != q.state {
        return (StatusCode::FORBIDDEN, "OAuth state mismatch").into_response();
    }

    let client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
    if client_id.is_empty() || client_secret.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Google OAuth is not configured",
        )
            .into_response();
    }
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

    let access_token = token_data
        .get("access_token")
        .and_then(|v| v.as_str())
        .unwrap_or("");
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
    let name = user_data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("User");

    let user = match state.auth_store.find_by_provider("google", google_id) {
        Ok(Some(u)) => u,
        _ => {
            // No Google-linked account. If this email already has a
            // password/magic account, link Google to it instead of
            // dead-ending in EMAIL_EXISTS — one person, one account.
            let existing = email.and_then(|e| state.auth_store.find_by_email(e).ok().flatten());
            match existing {
                Some(u) => {
                    if let Err(e) = state.auth_store.link_provider(&u.id, "google", google_id) {
                        eprintln!("[auth] google link failed: {e}");
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"error":"INTERNAL"})),
                        )
                            .into_response();
                    }
                    u
                }
                None => {
                    match state
                        .auth_store
                        .create_user(email, None, name, "google", Some(google_id))
                    {
                        Ok(u) => u,
                        Err(_) => {
                            return (StatusCode::CONFLICT, Json(json!({"error":"EMAIL_EXISTS"})))
                                .into_response()
                        }
                    }
                }
            }
        }
    };

    let session = match state.auth_store.create_session(&user.id) {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"INTERNAL"})),
            )
                .into_response()
        }
    };

    let cookie = Cookie::build(("session_token", session.token))
        .path("/")
        .http_only(true)
        .secure(false)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    (
        jar.add(cookie),
        Redirect::to(&format!("{base_url}/?auth_done=1")),
    )
        .into_response()
}
fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        email: row.get(1)?,
        phone: row.get(2)?,
        display_name: row.get(3)?,
        avatar_url: row.get(4)?,
        provider: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn unix_now() -> String {
    chrono::Utc::now().timestamp().to_string()
}
fn unix_after(secs: i64) -> String {
    (chrono::Utc::now().timestamp() + secs).to_string()
}

// ── Password hashing ─────────────────────────────────

fn hash_password(pw: &str) -> String {
    use argon2::{
        password_hash::{PasswordHasher, SaltString},
        Argon2,
    };
    use rand_core::OsRng;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn verify_password_hash(pw: &str, hash: &str) -> bool {
    use argon2::{password_hash::PasswordVerifier, Argon2};
    let parsed = argon2::PasswordHash::new(hash).ok();
    parsed
        .map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

// ── Register / Login ────────────────────────────────────

use serde_json::json;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}
#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Build the session cookie set on every successful sign-in. The frontend
/// authenticates by calling `/api/auth/me`, which reads this cookie — so
/// register/login/verify/oauth must all set it, not just return the token.
pub fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build(("session_token", token))
        .path("/")
        .http_only(true)
        .secure(false) // TODO: true in production
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build()
}

pub async fn handle_register(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    if !state
        .rate_limiter
        .allow(&format!("register:{}", body.email.to_lowercase()), 10, 900)
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"RATE_LIMITED"})),
        )
            .into_response();
    }
    if body.email.is_empty() || !body.email.contains('@') {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"INVALID_EMAIL"})),
        )
            .into_response();
    }
    if body.password.len() < 8 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"WEAK_PASSWORD"})),
        )
            .into_response();
    }
    match state.auth_store.create_user_with_password(
        &body.email,
        &body.password,
        &body.display_name,
    ) {
        Ok(user) => {
            let session = match state.auth_store.create_session(&user.id) {
                Ok(s) => s,
                Err(_) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error":"INTERNAL"})),
                    )
                        .into_response()
                }
            };
            let cookie = session_cookie(session.token.clone());
            (
                jar.add(cookie),
                Json(AuthResponse {
                    user,
                    token: session.token,
                }),
            )
                .into_response()
        }
        Err(_) => (StatusCode::CONFLICT, Json(json!({"error":"EMAIL_EXISTS"}))).into_response(),
    }
}

pub async fn handle_login(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    if !state
        .rate_limiter
        .allow(&format!("login:{}", body.email.to_lowercase()), 10, 900)
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"RATE_LIMITED"})),
        )
            .into_response();
    }
    match state
        .auth_store
        .verify_password(&body.email, &body.password)
    {
        Ok(Some(user)) => {
            let session = match state.auth_store.create_session(&user.id) {
                Ok(s) => s,
                Err(_) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error":"INTERNAL"})),
                    )
                        .into_response()
                }
            };
            let cookie = session_cookie(session.token.clone());
            (
                jar.add(cookie),
                Json(AuthResponse {
                    user,
                    token: session.token,
                }),
            )
                .into_response()
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"INVALID_CREDENTIALS"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetPasswordRequest {
    pub password: String,
}

/// Set a new password for the signed-in user (session required). The
/// recovery flow is: forgot password → magic link login → this endpoint.
/// An 8-char minimum keeps it consistent with registration.
pub async fn handle_set_password(
    State(state): State<crate::state::AppState>,
    jar: CookieJar,
    Json(body): Json<SetPasswordRequest>,
) -> impl IntoResponse {
    let token = jar.get("session_token").map(|c| c.value().to_string());
    let user = match token {
        Some(t) => state.auth_store.validate_session(&t),
        None => Ok(None),
    };
    let Ok(Some(user)) = user else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"NOT_AUTHENTICATED"})),
        )
            .into_response();
    };
    if body.password.len() < 8 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"WEAK_PASSWORD"})),
        )
            .into_response();
    }
    match state.auth_store.set_password(&user.id, &body.password) {
        Ok(_) => (StatusCode::OK, Json(json!({"ok":true}))).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"INTERNAL"})),
        )
            .into_response(),
    }
}
