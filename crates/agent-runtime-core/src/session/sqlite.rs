//! SqliteSessionStore: file-backed session persistence using SQLite.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::snapshot::SessionSnapshot;
use super::store::{SessionError, SessionStore};

pub struct SqliteSessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let conn = Connection::open(path).map_err(|e| SessionError::Storage(e.to_string()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, SessionError> {
        let conn =
            Connection::open_in_memory().map_err(|e| SessionError::Storage(e.to_string()))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, SessionError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                session_id    TEXT PRIMARY KEY,
                schema_version TEXT NOT NULL,
                snapshot_json  TEXT NOT NULL,
                saved_at       INTEGER NOT NULL
            );",
        )
        .map_err(|e| SessionError::Storage(e.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError> {
        let json = serde_json::to_string(snapshot)?;
        let schema = snapshot.schema_version.clone();
        let id = session_id.to_string();
        let conn = Arc::clone(&self.conn);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        tokio::task::spawn_blocking(move || {
            let c = conn
                .lock()
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            c.execute(
                "INSERT OR REPLACE INTO sessions \
                 (session_id, schema_version, snapshot_json, saved_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, schema, json, now as i64],
            )
            .map_err(|e| SessionError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| SessionError::Storage(e.to_string()))?
    }

    async fn load(&self, session_id: &str) -> Result<Option<SessionSnapshot>, SessionError> {
        let id = session_id.to_string();
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let c = conn
                .lock()
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            let mut stmt = c
                .prepare("SELECT snapshot_json, schema_version FROM sessions WHERE session_id = ?1")
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            let mut rows = stmt
                .query(params![id])
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            match rows
                .next()
                .map_err(|e| SessionError::Storage(e.to_string()))?
            {
                None => Ok(None),
                Some(row) => {
                    let json: String = row
                        .get(0)
                        .map_err(|e| SessionError::Storage(e.to_string()))?;
                    let row_schema: String = row
                        .get(1)
                        .map_err(|e| SessionError::Storage(e.to_string()))?;
                    if row_schema != SessionSnapshot::CURRENT_SCHEMA_VERSION {
                        return Err(SessionError::SchemaMismatch {
                            expected: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
                            found: row_schema,
                        });
                    }
                    let snap: SessionSnapshot = serde_json::from_str(&json)?;
                    Ok(Some(snap))
                }
            }
        })
        .await
        .map_err(|e| SessionError::Storage(e.to_string()))?
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionError> {
        let id = session_id.to_string();
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let c = conn
                .lock()
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            c.execute("DELETE FROM sessions WHERE session_id = ?1", params![id])
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| SessionError::Storage(e.to_string()))?
    }

    async fn list(&self) -> Result<Vec<String>, SessionError> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let c = conn
                .lock()
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            let mut stmt = c
                .prepare("SELECT session_id FROM sessions ORDER BY saved_at DESC")
                .map_err(|e| SessionError::Storage(e.to_string()))?;
            let ids: Result<Vec<String>, _> = stmt
                .query_map([], |row| row.get(0))
                .map_err(|e| SessionError::Storage(e.to_string()))?
                .map(|r| r.map_err(|e| SessionError::Storage(e.to_string())))
                .collect();
            ids
        })
        .await
        .map_err(|e| SessionError::Storage(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{AgentConfig, RunId};
    use crate::session::SessionStore;

    fn make_snap(session_id: &str, step: u32) -> SessionSnapshot {
        SessionSnapshot {
            schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
            session_id: session_id.into(),
            run_id: RunId::new(),
            messages: vec![],
            step,
            budget_used: Default::default(),
            active_config: AgentConfig::builder("test").build().unwrap(),
        }
    }

    #[tokio::test]
    async fn sqlite_save_and_load_roundtrip() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        let snap = make_snap("sess-1", 7);
        store.save("sess-1", &snap).await.unwrap();
        let loaded = store.load("sess-1").await.unwrap().expect("some");
        assert_eq!(loaded.step, 7);
        assert_eq!(loaded.session_id, "sess-1");
    }

    #[tokio::test]
    async fn sqlite_schema_version_mismatch() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        let snap = make_snap("sv-test", 0);
        store.save("sv-test", &snap).await.unwrap();
        // Manually update schema_version to trigger mismatch on next load
        {
            let c = store.conn.lock().unwrap();
            c.execute(
                "UPDATE sessions SET schema_version = '0.0' WHERE session_id = 'sv-test'",
                [],
            )
            .unwrap();
        }
        let err = store.load("sv-test").await.unwrap_err();
        assert!(matches!(err, SessionError::SchemaMismatch { .. }));
    }

    #[tokio::test]
    async fn sqlite_delete() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        let snap = make_snap("del-me", 1);
        store.save("del-me", &snap).await.unwrap();
        store.delete("del-me").await.unwrap();
        assert!(store.load("del-me").await.unwrap().is_none());
        // Idempotent
        store.delete("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn sqlite_list() {
        let store = SqliteSessionStore::open_in_memory().unwrap();
        for id in &["a", "b", "c"] {
            store.save(id, &make_snap(id, 0)).await.unwrap();
        }
        let mut ids = store.list().await.unwrap();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn sqlite_concurrent_saves() {
        use std::sync::Arc as StdArc;
        let store = StdArc::new(SqliteSessionStore::open_in_memory().unwrap());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let s = StdArc::clone(&store);
                tokio::spawn(async move {
                    let id = format!("concurrent-{i}");
                    s.save(&id, &make_snap(&id, i as u32)).await.unwrap();
                })
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        let ids = store.list().await.unwrap();
        assert_eq!(ids.len(), 10);
    }
}
