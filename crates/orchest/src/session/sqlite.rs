//! SqliteSessionStore: file-backed session logs using SQLite.
//!
//! Metadata lives in `session_meta`; events live in `session_events` keyed by
//! `(session_id, seq)`, so a reader can resume from any sequence number.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};

use super::log::{check_sequence, SessionEvent};
use super::store::{SessionError, SessionMeta, SessionStore};

pub struct SqliteSessionStore {
    conn: Arc<Mutex<Connection>>,
}

fn storage(e: impl std::fmt::Display) -> SessionError {
    SessionError::Storage(e.to_string())
}

impl SqliteSessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let conn = Connection::open(path).map_err(storage)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, SessionError> {
        let conn = Connection::open_in_memory().map_err(storage)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, SessionError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_meta (
                session_id     TEXT PRIMARY KEY,
                schema_version TEXT NOT NULL,
                meta_json      TEXT NOT NULL,
                created_at     INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_events (
                session_id TEXT NOT NULL,
                seq        INTEGER NOT NULL,
                event_json TEXT NOT NULL,
                PRIMARY KEY (session_id, seq)
            );",
        )
        .map_err(storage)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    async fn with_conn<T, F>(&self, f: F) -> Result<T, SessionError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, SessionError> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut c = conn.lock().map_err(storage)?;
            f(&mut c)
        })
        .await
        .map_err(storage)?
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn put_meta(&self, meta: &SessionMeta) -> Result<(), SessionError> {
        let json = serde_json::to_string(meta)?;
        let meta = meta.clone();
        self.with_conn(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO session_meta \
                 (session_id, schema_version, meta_json, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    meta.session_id,
                    meta.schema_version,
                    json,
                    meta.created_at_ms as i64
                ],
            )
            .map_err(storage)?;
            Ok(())
        })
        .await
    }

    async fn get_meta(&self, session_id: &str) -> Result<Option<SessionMeta>, SessionError> {
        let id = session_id.to_string();
        let row: Option<(String, String)> = self
            .with_conn(move |c| {
                c.query_row(
                    "SELECT schema_version, meta_json FROM session_meta WHERE session_id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(storage)
            })
            .await?;
        let Some((schema_version, json)) = row else {
            return Ok(None);
        };
        if schema_version != SessionMeta::CURRENT_SCHEMA_VERSION {
            return Err(SessionError::SchemaMismatch {
                expected: SessionMeta::CURRENT_SCHEMA_VERSION.into(),
                found: schema_version,
            });
        }
        let meta: SessionMeta = serde_json::from_str(&json)?;
        Ok(Some(meta))
    }

    async fn append(&self, session_id: &str, events: &[SessionEvent]) -> Result<(), SessionError> {
        let rows: Vec<(u64, String)> = events
            .iter()
            .map(|e| Ok((e.seq, serde_json::to_string(e)?)))
            .collect::<Result<_, serde_json::Error>>()?;
        let events = events.to_vec();
        let id = session_id.to_string();
        self.with_conn(move |c| {
            let tx = c.transaction().map_err(storage)?;
            let last: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE session_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .map_err(storage)?;
            check_sequence(last as u64 + 1, &events)?;
            {
                let mut stmt = tx
                    .prepare(
                        "INSERT INTO session_events (session_id, seq, event_json) \
                         VALUES (?1, ?2, ?3)",
                    )
                    .map_err(storage)?;
                for (seq, json) in rows {
                    stmt.execute(params![id, seq as i64, json])
                        .map_err(storage)?;
                }
            }
            tx.commit().map_err(storage)?;
            Ok(())
        })
        .await
    }

    async fn read(
        &self,
        session_id: &str,
        from_seq: u64,
    ) -> Result<Vec<SessionEvent>, SessionError> {
        let id = session_id.to_string();
        let rows: Vec<String> = self
            .with_conn(move |c| {
                let mut stmt = c
                    .prepare(
                        "SELECT event_json FROM session_events \
                         WHERE session_id = ?1 AND seq >= ?2 ORDER BY seq",
                    )
                    .map_err(storage)?;
                let rows = stmt
                    .query_map(params![id, from_seq as i64], |row| row.get(0))
                    .map_err(storage)?
                    .collect::<Result<Vec<String>, _>>()
                    .map_err(storage)?;
                Ok(rows)
            })
            .await?;
        rows.iter()
            .map(|json| serde_json::from_str(json).map_err(SessionError::from))
            .collect()
    }

    /// Each `append` commits its own transaction, which SQLite makes durable
    /// at commit under its default `synchronous` setting, so there is nothing
    /// left to force here.
    async fn flush(&self, _session_id: &str) -> Result<(), SessionError> {
        Ok(())
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionError> {
        let id = session_id.to_string();
        self.with_conn(move |c| {
            let tx = c.transaction().map_err(storage)?;
            tx.execute(
                "DELETE FROM session_meta WHERE session_id = ?1",
                params![id],
            )
            .map_err(storage)?;
            tx.execute(
                "DELETE FROM session_events WHERE session_id = ?1",
                params![id],
            )
            .map_err(storage)?;
            tx.commit().map_err(storage)?;
            Ok(())
        })
        .await
    }

    async fn list(&self) -> Result<Vec<String>, SessionError> {
        self.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT session_id FROM session_meta \
                     UNION SELECT DISTINCT session_id FROM session_events",
                )
                .map_err(storage)?;
            let ids = stmt
                .query_map([], |row| row.get(0))
                .map_err(storage)?
                .collect::<Result<Vec<String>, _>>()
                .map_err(storage)?;
            Ok(ids)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, MediaSource, Message, Role};
    use crate::session::log::{SessionEventKind, SessionProjection};

    fn user_event(seq: u64, text: &str) -> SessionEvent {
        SessionEvent {
            seq,
            timestamp_ms: 0,
            kind: SessionEventKind::UserMessage {
                content: vec![ContentBlock::Text(text.into())],
            },
        }
    }

    #[tokio::test]
    async fn sqlite_append_and_read_roundtrip() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        store
            .append("s", &[user_event(1, "a"), user_event(2, "b")])
            .await
            .unwrap();
        store.flush("s").await.unwrap();
        let events = store.read("s", 1).await.unwrap();
        assert_eq!(events, vec![user_event(1, "a"), user_event(2, "b")]);
    }

    #[tokio::test]
    async fn sqlite_incremental_read_by_seq() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        for seq in 1..=5 {
            store
                .append("s", &[user_event(seq, &seq.to_string())])
                .await
                .unwrap();
        }
        let tail = store.read("s", 4).await.unwrap();
        assert_eq!(tail.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
    }

    #[tokio::test]
    async fn sqlite_append_rejects_sequence_conflict_atomically() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        store.append("s", &[user_event(1, "a")]).await.unwrap();
        let err = store
            .append("s", &[user_event(2, "b"), user_event(4, "c")])
            .await
            .unwrap_err();
        assert!(matches!(err, SessionError::SequenceConflict { .. }));
        assert_eq!(store.read("s", 1).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn sqlite_roundtrip_preserves_image_content_block() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        let content = vec![
            ContentBlock::Text("describe this".to_string()),
            ContentBlock::Image {
                source: MediaSource::Url {
                    url: "https://example.com/cat.png".to_string(),
                },
                detail: None,
            },
        ];
        let event = SessionEvent {
            seq: 1,
            timestamp_ms: 0,
            kind: SessionEventKind::UserMessage {
                content: content.clone(),
            },
        };
        store.append("img", &[event]).await.unwrap();
        let projection = SessionProjection::from_events(&store.read("img", 1).await.unwrap());
        assert_eq!(
            projection.messages,
            vec![Message {
                role: Role::User,
                content
            }]
        );
    }

    #[tokio::test]
    async fn sqlite_meta_roundtrip_and_schema_mismatch() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        store
            .put_meta(&SessionMeta::new("s", "agent"))
            .await
            .unwrap();
        assert_eq!(
            store.get_meta("s").await.unwrap().unwrap().agent_name,
            "agent"
        );
        assert!(store.read("s", 1).await.unwrap().is_empty());
        {
            let c = store.conn.lock().unwrap();
            c.execute(
                "UPDATE session_meta SET schema_version = '0.1' WHERE session_id = 's'",
                [],
            )
            .unwrap();
        }
        assert!(matches!(
            store.get_meta("s").await,
            Err(SessionError::SchemaMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn sqlite_delete_and_list() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        for id in ["a", "b", "c"] {
            store
                .put_meta(&SessionMeta::new(id, "agent"))
                .await
                .unwrap();
            store.append(id, &[user_event(1, id)]).await.unwrap();
        }
        let mut ids = store.list().await.unwrap();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
        store.delete("b").await.unwrap();
        assert!(store.get_meta("b").await.unwrap().is_none());
        assert!(store.read("b", 1).await.unwrap().is_empty());
        store.delete("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn sqlite_concurrent_appends_to_distinct_sessions() {
        let store = Arc::new(SqliteSessionStore::open_in_memory().unwrap());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let s = Arc::clone(&store);
                tokio::spawn(async move {
                    let id = format!("concurrent-{i}");
                    s.append(&id, &[user_event(1, "x")]).await.unwrap();
                })
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(store.list().await.unwrap().len(), 10);
    }
}
