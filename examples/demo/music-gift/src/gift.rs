//! Gift model + SQLite-backed store.

use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// A music gift: lyrics + metadata + (eventually) generated audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gift {
    pub id: String,
    pub kind: String,
    pub lyrics: Option<String>,
    pub meta: Value,
    pub audio_url: Option<String>,
    pub photos: Vec<String>,
    pub gen_handle: Option<String>,
    pub gen_status: Option<String>,
    pub creator_token: String,
    pub published: bool,
    pub likes: Vec<String>,
    pub created_at: String,
    pub published_at: Option<String>,
}

/// Thread-safe SQLite gift store.
#[derive(Clone)]
pub struct GiftStore {
    conn: Arc<Mutex<Connection>>,
}

impl GiftStore {
    /// Open (or create) the gift database at `path`, initializing the schema.
    pub fn open(path: &str) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS gifts (
                id           TEXT PRIMARY KEY,
                kind         TEXT NOT NULL,
                lyrics       TEXT,
                meta         TEXT NOT NULL,
                audio_url    TEXT,
                photos       TEXT NOT NULL DEFAULT '[]',
                gen_handle   TEXT,
                gen_status   TEXT,
                creator_token TEXT NOT NULL,
                published    INTEGER NOT NULL DEFAULT 1,
                likes        TEXT NOT NULL DEFAULT '[]',
                created_at   TEXT NOT NULL,
                published_at TEXT
            );",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create a new gift record.
    pub fn create(&self, gift: &Gift) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO gifts (id, kind, lyrics, meta, audio_url, photos, gen_handle,
             gen_status, creator_token, published, likes, created_at, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                gift.id,
                gift.kind,
                gift.lyrics,
                serde_json::to_string(&gift.meta)?,
                gift.audio_url,
                serde_json::to_string(&gift.photos)?,
                gift.gen_handle,
                gift.gen_status,
                gift.creator_token,
                gift.published as i32,
                serde_json::to_string(&gift.likes)?,
                gift.created_at,
                gift.published_at,
            ],
        )?;
        Ok(())
    }

    /// Fetch a gift by id.
    pub fn get(&self, id: &str) -> AppResult<Gift> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, lyrics, meta, audio_url, photos, gen_handle, gen_status,
             creator_token, published, likes, created_at, published_at
             FROM gifts WHERE id = ?1",
        )?;
        let gift = stmt
            .query_row(params![id], row_to_gift)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("gift {id} not found"))
                }
                other => AppError::Database(other.to_string()),
            })?;
        Ok(gift)
    }

    /// List all published gifts, newest first.
    pub fn list_published(&self) -> AppResult<Vec<Gift>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, lyrics, meta, audio_url, photos, gen_handle, gen_status,
             creator_token, published, likes, created_at, published_at
             FROM gifts WHERE published = 1 ORDER BY published_at DESC",
        )?;
        let gifts = stmt
            .query_map([], row_to_gift)?
            .filter_map(Result::ok)
            .collect();
        Ok(gifts)
    }

    /// Update the generation handle + status on a gift.
    pub fn update_gen(&self, id: &str, handle_json: &str, status: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = conn.execute(
            "UPDATE gifts SET gen_handle = ?2, gen_status = ?3 WHERE id = ?1",
            params![id, handle_json, status],
        )?;
        if rows == 0 {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Set the audio URL and mark generation as done.
    pub fn update_audio(&self, id: &str, audio_url: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = conn.execute(
            "UPDATE gifts SET audio_url = ?2, gen_status = 'done' WHERE id = ?1",
            params![id, audio_url],
        )?;
        if rows == 0 {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Mark generation as failed.
    pub fn mark_gen_failed(&self, id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = conn.execute(
            "UPDATE gifts SET gen_status = 'failed' WHERE id = ?1",
            params![id],
        )?;
        if rows == 0 {
            return Err(AppError::NotFound(format!("gift {id} not found")));
        }
        Ok(())
    }

    /// Add a like (idempotent by viewer id).
    pub fn like(&self, id: &str, viewer_id: &str) -> AppResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut gift = Self::get_inner(&conn, id)?;
        if !gift.likes.iter().any(|l| l == viewer_id) {
            gift.likes.push(viewer_id.to_string());
            conn.execute(
                "UPDATE gifts SET likes = ?2 WHERE id = ?1",
                params![id, serde_json::to_string(&gift.likes)?],
            )?;
        }
        Ok(gift.likes.len())
    }

    fn get_inner(conn: &Connection, id: &str) -> AppResult<Gift> {
        let mut stmt = conn.prepare(
            "SELECT id, kind, lyrics, meta, audio_url, photos, gen_handle, gen_status,
             creator_token, published, likes, created_at, published_at
             FROM gifts WHERE id = ?1",
        )?;
        stmt.query_row(params![id], row_to_gift)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("gift {id} not found"))
                }
                other => AppError::Database(other.to_string()),
            })
    }
}

/// Map a rusqlite row to a `Gift`.
fn row_to_gift(row: &rusqlite::Row<'_>) -> rusqlite::Result<Gift> {
    let meta_str: String = row.get(3)?;
    let photos_str: String = row.get(5)?;
    let likes_str: String = row.get(10)?;
    let meta: Value = serde_json::from_str(&meta_str).unwrap_or(Value::Null);
    let photos: Vec<String> = serde_json::from_str(&photos_str).unwrap_or_default();
    let likes: Vec<String> = serde_json::from_str(&likes_str).unwrap_or_default();

    Ok(Gift {
        id: row.get(0)?,
        kind: row.get(1)?,
        lyrics: row.get(2)?,
        meta,
        audio_url: row.get(4)?,
        photos,
        gen_handle: row.get(6)?,
        gen_status: row.get(7)?,
        creator_token: row.get(8)?,
        published: row.get::<_, i32>(9)? != 0,
        likes,
        created_at: row.get(11)?,
        published_at: row.get(12)?,
    })
}
