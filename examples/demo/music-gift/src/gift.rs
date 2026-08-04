//! Gift model + SQLite-backed store.

use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gift {
    pub id: String,
    pub kind: String,
    pub lyrics: Option<String>,
    /// Raw meta JSON as stored/sent on the wire. Read it through
    /// [`Gift::meta`] — never by string key.
    pub meta: Value,
    pub audio_url: Option<String>,
    pub cover_url: Option<String>,
    pub photos: Vec<String>,
    pub gen_handle: Option<String>,
    pub gen_status: Option<String>,
    /// The exact GenRequest JSON sent to the provider at submit time —
    /// the only way to see what the provider actually received when a song
    /// comes out wrong (inspect with sqlite3). Never serialized: gifts are
    /// publicly viewable by id, so this must not reach the API response.
    #[serde(skip_serializing, default)]
    pub gen_request: Option<String>,
    pub countdown_status: Option<String>,
    pub lrc: Option<String>,
    pub duration_secs: Option<f64>,
    /// Ownership proof. Never serialized: any viewer may GET a gift by id
    /// (that is how sharing works), so echoing the token would hand every
    /// viewer the ability to delete it. Returned once, at creation only.
    #[serde(skip_serializing, default)]
    pub creator_token: String,
    /// Account the gift belongs to (set at creation when logged in, or later
    /// via `POST /api/gift/claim`). A session matching `creator_id` is an
    /// alternative ownership proof to `creator_token` — that is what makes
    /// gifts manageable from any device after login.
    pub creator_id: Option<String>,
    pub published: bool,
    pub likes: Vec<String>,
    pub created_at: String,
    pub published_at: Option<String>,
}

impl Gift {
    /// Typed view over the raw `meta` JSON. Malformed or non-object meta
    /// degrades to all-defaults, matching the old per-key `.get()` reads.
    pub fn meta(&self) -> GiftMeta {
        GiftMeta::from_value(&self.meta)
    }
}

/// Typed view over `Gift.meta`.
///
/// The stored column and the wire format stay an untyped JSON object; this
/// struct is the canonical home for the known keys and their defaults, so
/// handlers and tools stop re-deriving them per call site. Unknown keys the
/// frontend sends (e.g. `gender`, `model`) are preserved verbatim in
/// [`GiftMeta::extra`], so meta round-trips losslessly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GiftMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vocal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl GiftMeta {
    /// Fallback music style when neither the user nor the LLM picked one.
    pub const DEFAULT_STYLE: &'static str = "healing and warm";
    /// Fallback vocal gender.
    pub const DEFAULT_VOCAL: &'static str = "female";
    /// Fallback UI/lyrics language.
    pub const DEFAULT_LANG: &'static str = "en";
    /// Fallback title for the generation request — an untitled gift still
    /// needs a name for the provider. (Display sites use empty instead.)
    pub const DEFAULT_SONG_TITLE: &'static str = "Gift Song";
    /// Fallback recipient name (countdown copy).
    pub const DEFAULT_NAME: &'static str = "Someone";

    /// Parse a typed view from the raw stored meta JSON.
    pub fn from_value(value: &Value) -> Self {
        Self::deserialize(value).unwrap_or_default()
    }

    pub fn style_or_default(&self) -> &str {
        self.style.as_deref().unwrap_or(Self::DEFAULT_STYLE)
    }

    pub fn vocal_or_default(&self) -> &str {
        self.vocal.as_deref().unwrap_or(Self::DEFAULT_VOCAL)
    }

    pub fn lang_or_default(&self) -> &str {
        self.lang.as_deref().unwrap_or(Self::DEFAULT_LANG)
    }

    pub fn title_or_default(&self) -> &str {
        self.title.as_deref().unwrap_or(Self::DEFAULT_SONG_TITLE)
    }

    pub fn name_or_default(&self) -> &str {
        self.name.as_deref().unwrap_or(Self::DEFAULT_NAME)
    }
}

/// Immutable snapshot of a gift at one successful generation completion.
/// The `gifts` row always mirrors the latest version; older rows here keep
/// previous lyrics/meta/assets reachable (and their audio files alive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftVersion {
    pub gift_id: String,
    pub version: i64,
    pub lyrics: Option<String>,
    pub meta: Value,
    pub audio_url: Option<String>,
    pub cover_url: Option<String>,
    pub lrc: Option<String>,
    pub duration_secs: Option<f64>,
    /// Debug-only, same rule as [`Gift::gen_request`]: never serialized into
    /// API responses. Never read by Rust code either — inspect with sqlite3.
    /// (Stored for post-hoc debugging of a specific version's provider call.)
    #[serde(skip_serializing, default)]
    #[allow(dead_code)]
    pub gen_request: Option<String>,
    pub created_at: String,
}

/// Partial field updates for [`GiftStore::update_fields`] — also the PATCH
/// request body. Each `None` leaves the stored value untouched.
#[derive(Debug, Default, Deserialize)]
pub struct GiftFieldUpdates {
    pub lyrics: Option<String>,
    pub title: Option<String>,
    pub style: Option<String>,
    pub vocal: Option<String>,
}

#[derive(Clone)]
pub struct GiftStore {
    conn: Arc<Mutex<Connection>>,
}

const SELECT_COLS: &str = "\
    SELECT id, kind, lyrics, meta, audio_url, cover_url, photos, gen_handle, \
           gen_status, gen_request, countdown_status, lrc, duration_secs, creator_token, \
           published, likes, created_at, published_at, creator_id FROM gifts";

impl GiftStore {
    pub fn open(path: &str) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        // Gift and auth stores are separate connections to the same file, so a
        // write on one briefly locks out the other. Wait up to 5s for the lock
        // instead of failing requests with `database is locked`. (Chosen over
        // journal_mode=WAL: one pragma, no persistent on-disk format change,
        // and a demo workload never has more than one writer.)
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS gifts (
                id              TEXT PRIMARY KEY,
                kind            TEXT NOT NULL,
                lyrics          TEXT,
                meta            TEXT NOT NULL,
                audio_url       TEXT,
                cover_url       TEXT,
                photos          TEXT NOT NULL DEFAULT '[]',
                gen_handle      TEXT,
                gen_status      TEXT,
                gen_request     TEXT,
                creator_token   TEXT NOT NULL,
                creator_id      TEXT,
                published       INTEGER NOT NULL DEFAULT 1,
                likes           TEXT NOT NULL DEFAULT '[]',
                created_at      TEXT NOT NULL,
                published_at    TEXT,
                countdown_status TEXT,
                lrc             TEXT,
                duration_secs   REAL
            );",
        )?;
        // Version snapshots: one row per successful generation completion.
        // The gifts row keeps mirroring the latest version, so existing reads
        // are untouched; regenerate appends a new row instead of overwriting.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS gift_versions (
                gift_id        TEXT NOT NULL,
                version        INTEGER NOT NULL,
                lyrics         TEXT,
                meta           TEXT NOT NULL,
                audio_url      TEXT,
                cover_url      TEXT,
                lrc            TEXT,
                duration_secs  REAL,
                gen_request    TEXT,
                created_at     TEXT NOT NULL,
                PRIMARY KEY (gift_id, version)
            );",
        )?;
        for col in [
            "countdown_status",
            "lrc",
            "duration_secs",
            "cover_url",
            "gen_request",
        ] {
            let _ = conn.execute(&format!("ALTER TABLE gifts ADD COLUMN {col} TEXT"), []);
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn create(&self, gift: &Gift) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO gifts (id, kind, lyrics, meta, audio_url, cover_url, photos, gen_handle, \
             gen_status, gen_request, countdown_status, lrc, duration_secs, creator_token, published, \
             likes, created_at, published_at, creator_id) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![
                gift.id,
                gift.kind,
                gift.lyrics,
                serde_json::to_string(&gift.meta)?,
                gift.audio_url,
                gift.cover_url,
                serde_json::to_string(&gift.photos)?,
                gift.gen_handle,
                gift.gen_status,
                gift.gen_request,
                gift.countdown_status,
                gift.lrc,
                gift.duration_secs,
                gift.creator_token,
                gift.published as i32,
                serde_json::to_string(&gift.likes)?,
                gift.created_at,
                gift.published_at,
                gift.creator_id,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> AppResult<Gift> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let sql = format!("{SELECT_COLS} WHERE id = ?1");
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_row(params![id], row_to_gift)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("gift {id} not found"))
                }
                other => AppError::Database(other.to_string()),
            })
    }

    pub fn list_published(&self) -> AppResult<Vec<Gift>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let sql = format!("{SELECT_COLS} WHERE published = 1 ORDER BY published_at DESC");
        let mut stmt = conn.prepare(&sql)?;
        let gifts: Vec<Gift> = stmt
            .query_map([], row_to_gift)?
            .filter_map(Result::ok)
            .collect();
        Ok(gifts)
    }

    /// Toggle listing on the public playlist. The gift stays reachable by id
    /// either way, so a link already sent to someone keeps working.
    pub fn set_published(&self, id: &str, published: bool, now: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let published_at: Option<&str> = if published { Some(now) } else { None };
        if conn.execute(
            "UPDATE gifts SET published=?2, published_at=?3 WHERE id=?1",
            params![id, published as i32, published_at],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute("DELETE FROM gifts WHERE id=?1", params![id])? == 0 {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn update_gen(&self, id: &str, handle_json: &str, status: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET gen_handle=?2, gen_status=?3 WHERE id=?1",
            params![id, handle_json, status],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Persist the exact wire request sent to the provider. Debug-only
    /// observability: inspect with sqlite3; never served over the API.
    pub fn set_gen_request(&self, id: &str, request_json: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET gen_request=?2 WHERE id=?1",
            params![id, request_json],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn update_audio(&self, id: &str, audio_url: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET audio_url=?2, gen_status='done' WHERE id=?1",
            params![id, audio_url],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Persist provider-supplied cover art (surfaced as a `role: Cover` gen
    /// asset). Separate from `update_audio`: the cover is decorative and its
    /// write is best-effort at the call site.
    pub fn update_cover(&self, id: &str, cover_url: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET cover_url=?2 WHERE id=?1",
            params![id, cover_url],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn mark_gen_failed(&self, id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET gen_status='failed' WHERE id=?1",
            params![id],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn update_countdown_status(&self, id: &str, status: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET countdown_status=?2 WHERE id=?1",
            params![id, status],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn update_lrc(&self, id: &str, lrc: &str, dur: Option<f64>) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET lrc=?2, duration_secs=?3 WHERE id=?1",
            params![id, lrc, dur],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Record degraded generation stages (e.g. "music_prompt") into the
    /// gift's meta under the `degraded` key, so the frontend can tell the
    /// user that quality steps were skipped for this run. Written on every
    /// (re)generation — an empty list clears any stale marker from a
    /// previous degraded attempt.
    pub fn set_meta_degraded(&self, id: &str, stages: &[String]) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let gift = Self::get_inner(&conn, id)?;
        let mut meta = gift.meta;
        if !meta.is_object() {
            meta = json!({});
        }
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("degraded".to_string(), json!(stages));
        }
        if conn.execute(
            "UPDATE gifts SET meta=?2 WHERE id=?1",
            params![id, serde_json::to_string(&meta)?],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// All gifts owned by an account, newest first — the cross-device view
    /// behind `GET /api/my-gifts`. Includes unpublished and unfinished ones.
    pub fn list_by_creator(&self, creator_id: &str) -> AppResult<Vec<Gift>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let sql = format!("{SELECT_COLS} WHERE creator_id = ?1 ORDER BY created_at DESC");
        let mut stmt = conn.prepare(&sql)?;
        let gifts: Vec<Gift> = stmt
            .query_map(params![creator_id], row_to_gift)?
            .filter_map(Result::ok)
            .collect();
        Ok(gifts)
    }

    /// Attach a gift to an account, proving ownership with its creator
    /// token. Idempotent; returns false when the token does not match.
    pub fn claim_to_creator(
        &self,
        id: &str,
        creator_token: &str,
        creator_id: &str,
    ) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let changed = conn.execute(
            "UPDATE gifts SET creator_id=?3 \
             WHERE id=?1 AND creator_token=?2 AND (creator_id IS NULL OR creator_id<>?3)",
            params![id, creator_token, creator_id],
        )?;
        Ok(changed > 0)
    }

    /// Link a gift to an authenticated user. Set at creation when a session
    /// is present; the token remains a valid ownership proof too, and a
    /// matching session grants the same rights (see `verify_creator`).
    pub fn update_creator_id(&self, id: &str, creator_id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET creator_id=?2 WHERE id=?1",
            params![id, creator_id],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    pub fn like(&self, id: &str, viewer_id: &str) -> AppResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut gift = Self::get_inner(&conn, id)?;
        if !gift.likes.iter().any(|l| l == viewer_id) {
            gift.likes.push(viewer_id.to_string());
            conn.execute(
                "UPDATE gifts SET likes=?2 WHERE id=?1",
                params![id, serde_json::to_string(&gift.likes)?],
            )?;
        }
        Ok(gift.likes.len())
    }

    /// Snapshot the gift row as a new version (`max(version) + 1`, first is
    /// 1). Called on every successful generation completion; returns the new
    /// version number.
    pub fn add_version(&self, gift_id: &str) -> AppResult<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let gift = Self::get_inner(&conn, gift_id)?;
        let next: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM gift_versions WHERE gift_id=?1",
            params![gift_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO gift_versions (gift_id, version, lyrics, meta, audio_url, cover_url, \
             lrc, duration_secs, gen_request, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                gift.id,
                next,
                gift.lyrics,
                serde_json::to_string(&gift.meta)?,
                gift.audio_url,
                gift.cover_url,
                gift.lrc,
                gift.duration_secs,
                gift.gen_request,
                unix_now(),
            ],
        )?;
        Ok(next)
    }

    /// All versions of a gift, newest first. Legacy gifts (created before
    /// versioning) have no rows: synthesize v1 from the gift row — but only
    /// when it has an `audio_url`, since a never-completed gift has no
    /// version to show.
    pub fn list_versions(&self, gift_id: &str) -> AppResult<Vec<GiftVersion>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT gift_id, version, lyrics, meta, audio_url, cover_url, lrc, duration_secs, \
             gen_request, created_at FROM gift_versions WHERE gift_id=?1 ORDER BY version DESC",
        )?;
        let versions: Vec<GiftVersion> = stmt
            .query_map(params![gift_id], row_to_version)?
            .filter_map(Result::ok)
            .collect();
        if !versions.is_empty() {
            return Ok(versions);
        }
        let gift = Self::get_inner(&conn, gift_id)?;
        if gift.audio_url.is_none() {
            return Ok(Vec::new());
        }
        Ok(vec![GiftVersion {
            gift_id: gift.id,
            version: 1,
            lyrics: gift.lyrics,
            meta: gift.meta,
            audio_url: gift.audio_url,
            cover_url: gift.cover_url,
            lrc: gift.lrc,
            duration_secs: gift.duration_secs,
            gen_request: gift.gen_request,
            created_at: gift.created_at,
        }])
    }

    /// Partial edit of the work fields: the `lyrics` column plus the
    /// `title`/`style`/`vocal` meta keys. Absent fields are left untouched.
    /// Pure edit — never triggers generation.
    pub fn update_fields(&self, id: &str, updates: &GiftFieldUpdates) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let gift = Self::get_inner(&conn, id)?;
        let new_lyrics = updates.lyrics.clone().or(gift.lyrics);
        let mut meta = gift.meta;
        if !meta.is_object() {
            meta = json!({});
        }
        if let Some(obj) = meta.as_object_mut() {
            for (key, value) in [
                ("title", &updates.title),
                ("style", &updates.style),
                ("vocal", &updates.vocal),
            ] {
                if let Some(v) = value {
                    obj.insert(key.to_string(), json!(v));
                }
            }
        }
        if conn.execute(
            "UPDATE gifts SET lyrics=?2, meta=?3 WHERE id=?1",
            params![id, new_lyrics, serde_json::to_string(&meta)?],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Clear all generation state so the gift can be submitted again. The old
    /// audio file is NOT deleted — earlier version rows still reference it.
    pub fn reset_for_regeneration(&self, id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if conn.execute(
            "UPDATE gifts SET gen_status=NULL, gen_handle=NULL, audio_url=NULL, cover_url=NULL, \
             lrc=NULL, duration_secs=NULL WHERE id=?1",
            params![id],
        )? == 0
        {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Audio filenames referenced by this gift's version rows, for delete
    /// cleanup. Same safety rule as `delete_gift`: strip the `/audio/`
    /// prefix and reject empty names, `..`, and path separators.
    pub fn version_audio_files(&self, gift_id: &str) -> AppResult<Vec<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT audio_url FROM gift_versions WHERE gift_id=?1")?;
        let files: Vec<String> = stmt
            .query_map(params![gift_id], |row| row.get::<_, Option<String>>(0))?
            .filter_map(Result::ok)
            .flatten()
            .filter_map(|url| {
                url.strip_prefix("/audio/").and_then(|file| {
                    (!file.is_empty() && !file.contains("..") && !file.contains('/'))
                        .then(|| file.to_string())
                })
            })
            .collect();
        Ok(files)
    }

    fn get_inner(conn: &Connection, id: &str) -> AppResult<Gift> {
        let sql = format!("{SELECT_COLS} WHERE id = ?1");
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_row(params![id], row_to_gift)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("gift {id} not found"))
                }
                other => AppError::Database(other.to_string()),
            })
    }
}

fn row_to_gift(row: &rusqlite::Row<'_>) -> rusqlite::Result<Gift> {
    let meta_str: String = row.get(3)?;
    let photos_str: String = row.get(6)?;
    let likes_str: String = row.get(15)?;
    let meta: Value = serde_json::from_str(&meta_str).unwrap_or(Value::Null);
    let photos: Vec<String> = serde_json::from_str(&photos_str).unwrap_or_default();
    let likes: Vec<String> = serde_json::from_str(&likes_str).unwrap_or_default();

    Ok(Gift {
        id: row.get(0)?,
        kind: row.get(1)?,
        lyrics: row.get(2)?,
        meta,
        audio_url: row.get(4)?,
        cover_url: row.get(5)?,
        photos,
        gen_handle: row.get(7)?,
        gen_status: row.get(8)?,
        gen_request: row.get(9)?,
        countdown_status: row.get(10)?,
        lrc: row.get(11)?,
        duration_secs: row.get(12)?,
        creator_token: row.get(13)?,
        published: row.get::<_, i32>(14)? != 0,
        likes,
        created_at: row.get(16)?,
        published_at: row.get(17)?,
        creator_id: row.get(18)?,
    })
}

fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<GiftVersion> {
    let meta_str: String = row.get(3)?;
    let meta: Value = serde_json::from_str(&meta_str).unwrap_or(Value::Null);
    Ok(GiftVersion {
        gift_id: row.get(0)?,
        version: row.get(1)?,
        lyrics: row.get(2)?,
        meta,
        audio_url: row.get(4)?,
        cover_url: row.get(5)?,
        lrc: row.get(6)?,
        duration_secs: row.get(7)?,
        gen_request: row.get(8)?,
        created_at: row.get(9)?,
    })
}

/// Unix timestamp string for "now" (version created_at; same format as
/// `Gift.created_at`).
fn unix_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn meta_accessors_apply_canonical_defaults() {
        let m = GiftMeta::from_value(&json!({}));
        assert_eq!(m.style_or_default(), GiftMeta::DEFAULT_STYLE);
        assert_eq!(m.vocal_or_default(), GiftMeta::DEFAULT_VOCAL);
        assert_eq!(m.lang_or_default(), GiftMeta::DEFAULT_LANG);
        assert_eq!(m.title_or_default(), GiftMeta::DEFAULT_SONG_TITLE);
        assert_eq!(m.name_or_default(), GiftMeta::DEFAULT_NAME);
        assert!(m.birthday.is_none());
    }

    #[test]
    fn meta_present_keys_win_over_defaults() {
        let m = GiftMeta::from_value(&json!({"style": "warm folk", "vocal": "male"}));
        assert_eq!(m.style_or_default(), "warm folk");
        assert_eq!(m.vocal_or_default(), "male");
    }

    /// Serializing a parsed view must reproduce the exact same JSON keys —
    /// known keys under their original names, unknown ones preserved verbatim.
    #[test]
    fn meta_round_trips_known_and_unknown_keys() {
        let raw = json!({
            "name": "Alice",
            "birthday": "05-12",
            "gender": "female",
            "model": "V5_5",
        });
        let m = GiftMeta::from_value(&raw);
        assert_eq!(m.name.as_deref(), Some("Alice"));
        assert_eq!(m.birthday.as_deref(), Some("05-12"));
        let out = serde_json::to_value(&m).unwrap();
        assert_eq!(out, raw);
    }

    /// Non-object meta degrades to defaults, like the old per-key `.get()`.
    #[test]
    fn meta_from_non_object_yields_defaults() {
        let m = GiftMeta::from_value(&json!("not an object"));
        assert_eq!(m.style_or_default(), GiftMeta::DEFAULT_STYLE);
    }

    /// Degraded stages are stored in the gift's meta so the frontend can
    /// show them; an empty list clears a stale marker from a previous run.
    /// Claiming is token-proof and idempotent; the account list then sees
    /// the gift (this is what makes Mine cross-device after login).
    #[test]
    fn claim_attaches_gift_to_account_and_list_by_creator_returns_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gifts.db");
        let store = GiftStore::open(&db.to_string_lossy()).expect("open store");
        let gift = Gift {
            id: "g1".to_string(),
            kind: "song".to_string(),
            lyrics: Some("la".to_string()),
            meta: json!({}),
            audio_url: None,
            cover_url: None,
            photos: vec![],
            gen_handle: None,
            gen_status: None,
            gen_request: None,
            countdown_status: None,
            lrc: None,
            duration_secs: None,
            creator_token: "tok".to_string(),
            creator_id: None,
            published: false,
            likes: vec![],
            created_at: "10".to_string(),
            published_at: None,
        };
        store.create(&gift).expect("create");

        // Wrong token: no attach.
        assert!(!store
            .claim_to_creator("g1", "wrong", "user-1")
            .expect("claim"));
        assert!(store.list_by_creator("user-1").expect("list").is_empty());

        // Right token: attached; idempotent on repeat.
        assert!(store
            .claim_to_creator("g1", "tok", "user-1")
            .expect("claim"));
        assert!(!store
            .claim_to_creator("g1", "tok", "user-1")
            .expect("claim again"));
        let mine = store.list_by_creator("user-1").expect("list");
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, "g1");
        assert_eq!(mine[0].creator_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn set_meta_degraded_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gifts.db");
        let store = GiftStore::open(&db.to_string_lossy()).expect("open store");
        let gift = Gift {
            id: "g1".to_string(),
            kind: "song".to_string(),
            lyrics: Some("la".to_string()),
            meta: json!({"name": "Alice"}),
            audio_url: None,
            cover_url: None,
            photos: vec![],
            gen_handle: None,
            gen_status: None,
            gen_request: None,
            countdown_status: None,
            lrc: None,
            duration_secs: None,
            creator_token: "tok".to_string(),
            creator_id: None,
            published: false,
            likes: vec![],
            created_at: "0".to_string(),
            published_at: None,
        };
        store.create(&gift).expect("create");

        store
            .set_meta_degraded("g1", &["music_prompt".to_string()])
            .expect("set degraded");
        let got = store.get("g1").expect("get");
        assert_eq!(got.meta["degraded"], json!(["music_prompt"]));
        // Existing keys survive the meta rewrite.
        assert_eq!(got.meta["name"], json!("Alice"));

        store.set_meta_degraded("g1", &[]).expect("clear degraded");
        assert_eq!(store.get("g1").expect("get").meta["degraded"], json!([]));
    }

    /// The exact wire payload is stored for debugging but must never leak
    /// into the API response (gifts are publicly viewable by id).
    #[test]
    fn gen_request_round_trips_and_stays_out_of_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gifts.db");
        let store = GiftStore::open(&db.to_string_lossy()).expect("open store");
        let gift = Gift {
            id: "g1".to_string(),
            kind: "song".to_string(),
            lyrics: None,
            meta: json!({}),
            audio_url: None,
            cover_url: None,
            photos: vec![],
            gen_handle: None,
            gen_status: None,
            gen_request: None,
            countdown_status: None,
            lrc: None,
            duration_secs: None,
            creator_token: "tok".to_string(),
            creator_id: None,
            published: false,
            likes: vec![],
            created_at: "0".to_string(),
            published_at: None,
        };
        store.create(&gift).expect("create");
        assert!(store.get("g1").expect("get").gen_request.is_none());

        let wire = json!({"prompt": "p", "music": {"style": "warm acoustic, indie folk"}});
        store
            .set_gen_request("g1", &wire.to_string())
            .expect("set gen_request");
        let got = store.get("g1").expect("get");
        assert_eq!(got.gen_request.as_deref(), Some(wire.to_string().as_str()));

        let json = serde_json::to_value(&got).expect("serialize");
        assert!(json.get("gen_request").is_none());
    }

    /// Minimal gift fixture for the versioning tests.
    fn test_gift(id: &str) -> Gift {
        Gift {
            id: id.to_string(),
            kind: "song".to_string(),
            lyrics: Some("la".to_string()),
            meta: json!({"title": "T", "style": "warm", "vocal": "female"}),
            audio_url: None,
            cover_url: None,
            photos: vec![],
            gen_handle: None,
            gen_status: None,
            gen_request: None,
            countdown_status: None,
            lrc: None,
            duration_secs: None,
            creator_token: "tok".to_string(),
            creator_id: None,
            published: false,
            likes: vec![],
            created_at: "100".to_string(),
            published_at: None,
        }
    }

    fn test_store(dir: &tempfile::TempDir) -> GiftStore {
        let db = dir.path().join("gifts.db");
        GiftStore::open(&db.to_string_lossy()).expect("open store")
    }

    /// The generation completion path (update_audio/cover/lrc, then
    /// add_version) snapshots the gift row as v1.
    #[test]
    fn completion_path_writes_version_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        store.create(&test_gift("g1")).expect("create");

        store
            .update_audio("g1", "/audio/a.mp3")
            .expect("update_audio");
        store
            .update_cover("g1", "https://cover")
            .expect("update_cover");
        store
            .update_lrc("g1", "[00:01.00]la", Some(3.5))
            .expect("update_lrc");
        store
            .set_gen_request("g1", "{\"prompt\":\"p\"}")
            .expect("set_gen_request");
        let v = store.add_version("g1").expect("add_version");
        assert_eq!(v, 1);

        let versions = store.list_versions("g1").expect("list");
        assert_eq!(versions.len(), 1);
        let v1 = &versions[0];
        assert_eq!(v1.version, 1);
        assert_eq!(v1.lyrics.as_deref(), Some("la"));
        assert_eq!(v1.meta["title"], json!("T"));
        assert_eq!(v1.audio_url.as_deref(), Some("/audio/a.mp3"));
        assert_eq!(v1.cover_url.as_deref(), Some("https://cover"));
        assert_eq!(v1.lrc.as_deref(), Some("[00:01.00]la"));
        assert_eq!(v1.duration_secs, Some(3.5));
        assert_eq!(v1.gen_request.as_deref(), Some("{\"prompt\":\"p\"}"));
    }

    /// Legacy gifts (no version rows) synthesize v1 from the gift row —
    /// only when the gift actually completed a generation (has audio).
    #[test]
    fn legacy_gift_synthesizes_v1() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        let mut gift = test_gift("g1");
        gift.audio_url = Some("/audio/old.mp3".to_string());
        store.create(&gift).expect("create");

        let versions = store.list_versions("g1").expect("list");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, 1);
        assert_eq!(versions[0].audio_url.as_deref(), Some("/audio/old.mp3"));
        assert_eq!(versions[0].created_at, "100");
    }

    #[test]
    fn never_completed_gift_has_no_versions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        store.create(&test_gift("g1")).expect("create");
        assert!(store.list_versions("g1").expect("list").is_empty());
    }

    /// Newest first; version numbers increment per completion.
    #[test]
    fn versions_list_descending() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        store.create(&test_gift("g1")).expect("create");
        store
            .update_audio("g1", "/audio/a.mp3")
            .expect("update_audio");
        assert_eq!(store.add_version("g1").expect("v1"), 1);
        store
            .update_audio("g1", "/audio/b.mp3")
            .expect("update_audio");
        assert_eq!(store.add_version("g1").expect("v2"), 2);

        let versions = store.list_versions("g1").expect("list");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 2);
        assert_eq!(versions[0].audio_url.as_deref(), Some("/audio/b.mp3"));
        assert_eq!(versions[1].version, 1);
        assert_eq!(versions[1].audio_url.as_deref(), Some("/audio/a.mp3"));
    }

    /// PATCH semantics: absent fields are left untouched.
    #[test]
    fn update_fields_partial_update() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        store.create(&test_gift("g1")).expect("create");

        store
            .update_fields(
                "g1",
                &GiftFieldUpdates {
                    title: Some("New Title".to_string()),
                    ..Default::default()
                },
            )
            .expect("patch title");
        let got = store.get("g1").expect("get");
        assert_eq!(got.meta["title"], json!("New Title"));
        assert_eq!(got.lyrics.as_deref(), Some("la"));
        assert_eq!(got.meta["style"], json!("warm"));
        assert_eq!(got.meta["vocal"], json!("female"));

        store
            .update_fields(
                "g1",
                &GiftFieldUpdates {
                    lyrics: Some("new lyrics".to_string()),
                    style: Some("rock".to_string()),
                    vocal: Some("male".to_string()),
                    ..Default::default()
                },
            )
            .expect("patch rest");
        let got = store.get("g1").expect("get");
        assert_eq!(got.lyrics.as_deref(), Some("new lyrics"));
        assert_eq!(got.meta["style"], json!("rock"));
        assert_eq!(got.meta["vocal"], json!("male"));
        assert_eq!(got.meta["title"], json!("New Title"));
    }

    /// Regenerate resets all six generation fields to NULL; lyrics/meta and
    /// the version rows survive.
    #[test]
    fn reset_for_regeneration_clears_generation_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        store.create(&test_gift("g1")).expect("create");
        store.update_gen("g1", "handle-json", "done").expect("gen");
        store
            .update_audio("g1", "/audio/a.mp3")
            .expect("update_audio");
        store
            .update_cover("g1", "https://cover")
            .expect("update_cover");
        store
            .update_lrc("g1", "[00:01.00]la", Some(3.5))
            .expect("update_lrc");
        store.add_version("g1").expect("add_version");

        store.reset_for_regeneration("g1").expect("reset");
        let got = store.get("g1").expect("get");
        assert!(got.gen_status.is_none());
        assert!(got.gen_handle.is_none());
        assert!(got.audio_url.is_none());
        assert!(got.cover_url.is_none());
        assert!(got.lrc.is_none());
        assert!(got.duration_secs.is_none());
        // Edit fields and prior versions are untouched by the reset.
        assert_eq!(got.lyrics.as_deref(), Some("la"));
        assert_eq!(store.list_versions("g1").expect("list").len(), 1);
    }

    /// Version rows yield cleaned audio filenames for delete cleanup, using
    /// the same safety rule as delete_gift.
    #[test]
    fn version_audio_files_filters_unsafe_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = test_store(&dir);
        store.create(&test_gift("g1")).expect("create");
        store
            .update_audio("g1", "/audio/a.mp3")
            .expect("update_audio");
        store.add_version("g1").expect("v1");
        // Malicious/odd values must be dropped.
        let conn = store.conn.lock().expect("lock");
        conn.execute(
            "INSERT INTO gift_versions (gift_id, version, lyrics, meta, audio_url, created_at) \
             VALUES ('g1', 2, NULL, '{}', '/audio/../secret', '1')",
            [],
        )
        .expect("insert bad");
        drop(conn);

        let files = store.version_audio_files("g1").expect("files");
        assert_eq!(files, vec!["a.mp3".to_string()]);
    }

    /// gen_request is debug-only and must never serialize into API responses
    /// (same rule as Gift::gen_request).
    #[test]
    fn gift_version_gen_request_stays_out_of_json() {
        let v = GiftVersion {
            gift_id: "g1".to_string(),
            version: 1,
            lyrics: None,
            meta: json!({}),
            audio_url: Some("/audio/a.mp3".to_string()),
            cover_url: None,
            lrc: None,
            duration_secs: None,
            gen_request: Some("{\"prompt\":\"p\"}".to_string()),
            created_at: "1".to_string(),
        };
        let json = serde_json::to_value(&v).expect("serialize");
        assert!(json.get("gen_request").is_none());
        assert_eq!(json["audio_url"], json!("/audio/a.mp3"));
    }
}
