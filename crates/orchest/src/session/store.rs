//! SessionStore trait + SessionError + InMemorySessionStore.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::snapshot::SessionSnapshot;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("schema version mismatch: expected {expected}, got {found}")]
    SchemaMismatch { expected: String, found: String },
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError>;
    async fn load(&self, session_id: &str) -> Result<Option<SessionSnapshot>, SessionError>;
    async fn delete(&self, session_id: &str) -> Result<(), SessionError>;
    async fn list(&self) -> Result<Vec<String>, SessionError>;
}

#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: Arc<Mutex<HashMap<String, SessionSnapshot>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError> {
        // JSON round-trip enforces the same semantics as persistent backends
        // (serde(skip) fields are dropped; schema_version is preserved).
        let json = serde_json::to_string(snapshot)?;
        let parsed: SessionSnapshot = serde_json::from_str(&json)?;
        self.sessions
            .lock()
            .await
            .insert(session_id.to_string(), parsed);
        Ok(())
    }

    async fn load(&self, session_id: &str) -> Result<Option<SessionSnapshot>, SessionError> {
        let guard = self.sessions.lock().await;
        match guard.get(session_id) {
            None => Ok(None),
            Some(snap) => {
                if snap.schema_version != SessionSnapshot::CURRENT_SCHEMA_VERSION {
                    return Err(SessionError::SchemaMismatch {
                        expected: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
                        found: snap.schema_version.clone(),
                    });
                }
                Ok(Some(snap.clone()))
            }
        }
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionError> {
        self.sessions.lock().await.remove(session_id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>, SessionError> {
        Ok(self.sessions.lock().await.keys().cloned().collect())
    }
}
