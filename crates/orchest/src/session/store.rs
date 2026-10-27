//! SessionStore trait + SessionError + SessionMeta + InMemorySessionStore.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::log::{check_sequence, SessionEvent};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("schema version mismatch: expected {expected}, got {found}")]
    SchemaMismatch { expected: String, found: String },
    /// Appended events do not continue the stored log.
    #[error("session log sequence conflict: expected seq {expected}, got {found}")]
    SequenceConflict { expected: u64, found: u64 },
    /// No metadata or events are stored under this session id.
    #[error("session '{0}' not found")]
    NotFound(String),
    /// A fresh run was pointed at a session that already holds events.
    #[error("session '{0}' already exists; continue it with AgentRun::resume")]
    AlreadyExists(String),
    /// Resume was called on a config without a session store and session id.
    #[error("no session store and session id are configured")]
    NotConfigured,
}

/// When a run makes its session log durable.
///
/// Every policy appends and flushes at the start of each step (before the
/// model request), at the end of each step, at run termination, and before
/// dispatching a tool that has side effects. `EveryTool` also flushes before
/// every other tool dispatch and after every tool result, trading more writes
/// for a smaller loss window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CheckpointPolicy {
    #[default]
    EveryStep,
    EveryTool,
}

/// Session metadata, stored apart from the event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub schema_version: String,
    pub session_id: String,
    /// Milliseconds since the Unix epoch.
    pub created_at_ms: u64,
    /// The agent the session was created for.
    pub agent_name: String,
    /// The checkpoint policy the session is written under.
    pub checkpoint_policy: CheckpointPolicy,
}

impl SessionMeta {
    pub const CURRENT_SCHEMA_VERSION: &'static str = "2";

    pub fn new(session_id: impl Into<String>, agent_name: impl Into<String>) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION.into(),
            session_id: session_id.into(),
            created_at_ms: super::log::now_ms(),
            agent_name: agent_name.into(),
            checkpoint_policy: CheckpointPolicy::default(),
        }
    }

    pub fn with_checkpoint_policy(mut self, policy: CheckpointPolicy) -> Self {
        self.checkpoint_policy = policy;
        self
    }

    pub(crate) fn check_schema(&self) -> Result<(), SessionError> {
        if self.schema_version != Self::CURRENT_SCHEMA_VERSION {
            return Err(SessionError::SchemaMismatch {
                expected: Self::CURRENT_SCHEMA_VERSION.into(),
                found: self.schema_version.clone(),
            });
        }
        Ok(())
    }
}

/// Durable home of session event logs. The host implements it on its own
/// storage; Orchest ships an in-memory and a SQLite backend.
///
/// Durability contract: `append` only promises that events are accepted in
/// order. `flush` is the durability barrier — once it returns, every event
/// accepted before it survives a process crash. A backend may be stronger
/// (for example durable on every append), but callers rely on `flush` only.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Writes the metadata of a session, replacing any previous value.
    async fn put_meta(&self, meta: &SessionMeta) -> Result<(), SessionError>;
    /// Reads the metadata of a session.
    async fn get_meta(&self, session_id: &str) -> Result<Option<SessionMeta>, SessionError>;
    /// Accepts `events`, which must continue the stored log: the first event's
    /// `seq` is one past the last stored event (1 for an empty log) and the
    /// rest are consecutive. Anything else is a
    /// [`SessionError::SequenceConflict`] and nothing is accepted.
    async fn append(&self, session_id: &str, events: &[SessionEvent]) -> Result<(), SessionError>;
    /// Reads the events with `seq >= from_seq`, in order.
    async fn read(
        &self,
        session_id: &str,
        from_seq: u64,
    ) -> Result<Vec<SessionEvent>, SessionError>;
    /// Durability barrier for everything accepted so far.
    async fn flush(&self, session_id: &str) -> Result<(), SessionError>;
    /// Removes a session's metadata and events. Deleting an unknown session
    /// is not an error.
    async fn delete(&self, session_id: &str) -> Result<(), SessionError>;
    /// Lists stored session ids.
    async fn list(&self) -> Result<Vec<String>, SessionError>;
}

#[derive(Default)]
struct InMemorySession {
    meta: Option<SessionMeta>,
    events: Vec<SessionEvent>,
}

#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: Arc<Mutex<HashMap<String, InMemorySession>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn put_meta(&self, meta: &SessionMeta) -> Result<(), SessionError> {
        self.sessions
            .lock()
            .await
            .entry(meta.session_id.clone())
            .or_default()
            .meta = Some(meta.clone());
        Ok(())
    }

    async fn get_meta(&self, session_id: &str) -> Result<Option<SessionMeta>, SessionError> {
        let guard = self.sessions.lock().await;
        let Some(meta) = guard.get(session_id).and_then(|s| s.meta.clone()) else {
            return Ok(None);
        };
        meta.check_schema()?;
        Ok(Some(meta))
    }

    async fn append(&self, session_id: &str, events: &[SessionEvent]) -> Result<(), SessionError> {
        // JSON round-trip enforces the same semantics as persistent backends.
        let json = serde_json::to_string(events)?;
        let parsed: Vec<SessionEvent> = serde_json::from_str(&json)?;
        let mut guard = self.sessions.lock().await;
        let session = guard.entry(session_id.to_string()).or_default();
        check_sequence(session.events.len() as u64 + 1, &parsed)?;
        session.events.extend(parsed);
        Ok(())
    }

    async fn read(
        &self,
        session_id: &str,
        from_seq: u64,
    ) -> Result<Vec<SessionEvent>, SessionError> {
        let guard = self.sessions.lock().await;
        Ok(guard
            .get(session_id)
            .map(|s| {
                s.events
                    .iter()
                    .filter(|e| e.seq >= from_seq)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn flush(&self, _session_id: &str) -> Result<(), SessionError> {
        Ok(())
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionError> {
        self.sessions.lock().await.remove(session_id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>, SessionError> {
        Ok(self.sessions.lock().await.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ContentBlock;
    use crate::session::log::SessionEventKind;

    fn user_event(seq: u64) -> SessionEvent {
        SessionEvent {
            seq,
            timestamp_ms: 0,
            kind: SessionEventKind::UserMessage {
                content: vec![ContentBlock::Text(format!("m{seq}"))],
            },
        }
    }

    #[tokio::test]
    async fn append_read_and_incremental_read() {
        let store = InMemorySessionStore::new();
        store
            .append("s", &[user_event(1), user_event(2)])
            .await
            .unwrap();
        store.append("s", &[user_event(3)]).await.unwrap();
        store.flush("s").await.unwrap();
        assert_eq!(store.read("s", 1).await.unwrap().len(), 3);
        let tail = store.read("s", 3).await.unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].seq, 3);
        assert!(store.read("missing", 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn append_rejects_non_continuing_events() {
        let store = InMemorySessionStore::new();
        store.append("s", &[user_event(1)]).await.unwrap();
        let err = store.append("s", &[user_event(3)]).await.unwrap_err();
        assert!(matches!(
            err,
            SessionError::SequenceConflict {
                expected: 2,
                found: 3
            }
        ));
        assert!(store.append("s", &[user_event(1)]).await.is_err());
        assert_eq!(store.read("s", 1).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn meta_is_separate_from_events() {
        let store = InMemorySessionStore::new();
        store
            .put_meta(&SessionMeta::new("s", "agent"))
            .await
            .unwrap();
        assert!(store.read("s", 1).await.unwrap().is_empty());
        let meta = store.get_meta("s").await.unwrap().unwrap();
        assert_eq!(meta.agent_name, "agent");
        assert_eq!(meta.schema_version, SessionMeta::CURRENT_SCHEMA_VERSION);
        assert_eq!(store.list().await.unwrap(), vec!["s".to_string()]);
        store.delete("s").await.unwrap();
        assert!(store.get_meta("s").await.unwrap().is_none());
        store.delete("s").await.unwrap();
    }

    #[tokio::test]
    async fn meta_schema_mismatch_is_rejected() {
        let store = InMemorySessionStore::new();
        let mut meta = SessionMeta::new("s", "agent");
        meta.schema_version = "0.1".into();
        store.put_meta(&meta).await.unwrap();
        assert!(matches!(
            store.get_meta("s").await,
            Err(SessionError::SchemaMismatch { .. })
        ));
    }
}
