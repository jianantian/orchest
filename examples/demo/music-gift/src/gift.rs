//! Gift model + SQLite-backed store.

use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub countdown_status: Option<String>,
    pub lrc: Option<String>,
    pub duration_secs: Option<f64>,
    /// Ownership proof. Never serialized: any viewer may GET a gift by id
    /// (that is how sharing works), so echoing the token would hand every
    /// viewer the ability to delete it. Returned once, at creation only.
    #[serde(skip_serializing, default)]
    pub creator_token: String,
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

#[derive(Clone)]
pub struct GiftStore {
    conn: Arc<Mutex<Connection>>,
}

const SELECT_COLS: &str = "\
    SELECT id, kind, lyrics, meta, audio_url, cover_url, photos, gen_handle, \
           gen_status, countdown_status, lrc, duration_secs, creator_token, \
           published, likes, created_at, published_at FROM gifts";

impl GiftStore {
    pub fn open(path: &str) -> AppResult<Self> {
        let conn = Connection::open(path)?;
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
                creator_token   TEXT NOT NULL,
                published       INTEGER NOT NULL DEFAULT 1,
                likes           TEXT NOT NULL DEFAULT '[]',
                created_at      TEXT NOT NULL,
                published_at    TEXT,
                countdown_status TEXT,
                lrc             TEXT,
                duration_secs   REAL
            );",
        )?;
        for col in ["countdown_status", "lrc", "duration_secs", "cover_url"] {
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
             gen_status, countdown_status, lrc, duration_secs, creator_token, published, \
             likes, created_at, published_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
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
                gift.countdown_status,
                gift.lrc,
                gift.duration_secs,
                gift.creator_token,
                gift.published as i32,
                serde_json::to_string(&gift.likes)?,
                gift.created_at,
                gift.published_at,
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

    /// Link a gift to an authenticated user. Purely additive metadata: the
    /// `creator_token` remains the only mutation check, and gifts created
    /// without a session keep `creator_id` NULL.
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
    let likes_str: String = row.get(14)?;
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
        countdown_status: row.get(9)?,
        lrc: row.get(10)?,
        duration_secs: row.get(11)?,
        creator_token: row.get(12)?,
        published: row.get::<_, i32>(13)? != 0,
        likes,
        created_at: row.get(15)?,
        published_at: row.get(16)?,
    })
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
}
