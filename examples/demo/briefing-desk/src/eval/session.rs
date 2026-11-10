//! Follow-up session seed materialization with per-attempt isolation.
//!
//! Each attempt materializes the seed as a session log with unique
//! session/run IDs in its own temporary SQLite store. Cleanup results are
//! recorded for `attempt.json`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use orchest::run::{AgentConfig, RunId};
use orchest::session::{
    SessionEventKind, SessionLog, SessionMeta, SessionStore, SqliteSessionStore,
};
use tempfile::TempDir;

use super::artifact::{ArtifactError, StoreCleanupRecord};
use super::case::{normalize_seed_bytes, SessionSeed};

/// Materialized attempt session with cleanup guard.
pub struct AttemptSession {
    pub session_id: String,
    pub run_id: RunId,
    pub seed_id: String,
    pub seed_hash: String,
    /// The seed as a session log, as stored.
    pub log: SessionLog,
    pub store: Arc<dyn SessionStore>,
    pub store_path: PathBuf,
    config: AgentConfig,
    tempdir: Option<TempDir>,
    cleaned: bool,
}

/// The session log a seed stands for: its messages, and a closed step
/// boundary carrying its step count and budget.
fn seed_log(seed: &SessionSeed, run_id: RunId, agent_name: &str) -> SessionLog {
    let mut log = SessionLog::new();
    log.append(SessionEventKind::RunStarted {
        run_id,
        agent_name: agent_name.to_string(),
    });
    for message in seed.messages.iter().cloned() {
        log.append_message(message);
    }
    if seed.step > 0 {
        log.append(SessionEventKind::StepEnded {
            step: seed.step - 1,
            budget_used: seed.budget_used.clone(),
        });
    }
    log
}

struct Prepared {
    session_id: String,
    run_id: RunId,
    seed_hash: String,
    log: SessionLog,
    store: Arc<dyn SessionStore>,
    store_path: PathBuf,
    config: AgentConfig,
    tempdir: TempDir,
}

fn prepare(seed: &SessionSeed, base_config: AgentConfig) -> Result<Prepared, ArtifactError> {
    let seed_hash = seed
        .content_hash()
        .map_err(|e| ArtifactError::new(e.to_string()))?;
    let session_id = format!("eval-{}-{}", seed.seed_id, unique_suffix());
    let run_id = RunId::new();

    let tempdir =
        TempDir::new().map_err(|e| ArtifactError::io(format!("creating temp session dir: {e}")))?;
    let store_path = tempdir.path().join("session.sqlite3");
    let store = SqliteSessionStore::open(&store_path)
        .map_err(|e| ArtifactError::io(format!("opening temp session store: {e}")))?;
    let store: Arc<dyn SessionStore> = Arc::new(store);

    let log = seed_log(seed, run_id, &base_config.name);
    let config = base_config.with_session_store(Arc::clone(&store), session_id.clone());
    Ok(Prepared {
        session_id,
        run_id,
        seed_hash,
        log,
        store,
        store_path,
        config,
        tempdir,
    })
}

async fn write_seed(prepared: &Prepared) -> Result<(), ArtifactError> {
    let save = |e| ArtifactError::io(format!("saving materialized seed: {e}"));
    prepared
        .store
        .put_meta(&SessionMeta::new(
            &prepared.session_id,
            &prepared.config.name,
        ))
        .await
        .map_err(save)?;
    prepared
        .store
        .append(&prepared.session_id, prepared.log.events())
        .await
        .map_err(save)?;
    prepared
        .store
        .flush(&prepared.session_id)
        .await
        .map_err(save)
}

impl AttemptSession {
    /// Materialize a validated seed into a unique session log + temp SQLite store.
    pub fn materialize(
        seed: &SessionSeed,
        base_config: AgentConfig,
    ) -> Result<Self, ArtifactError> {
        let prepared = prepare(seed, base_config)?;
        block_on_store(write_seed(&prepared))?;
        Ok(Self::from_prepared(seed, prepared))
    }

    /// Async materialize that writes via the store's async API.
    pub async fn materialize_async(
        seed: &SessionSeed,
        base_config: AgentConfig,
    ) -> Result<Self, ArtifactError> {
        let prepared = prepare(seed, base_config)?;
        write_seed(&prepared).await?;
        Ok(Self::from_prepared(seed, prepared))
    }

    fn from_prepared(seed: &SessionSeed, prepared: Prepared) -> Self {
        Self {
            session_id: prepared.session_id,
            run_id: prepared.run_id,
            seed_id: seed.seed_id.clone(),
            seed_hash: prepared.seed_hash,
            log: prepared.log,
            store: prepared.store,
            store_path: prepared.store_path,
            config: prepared.config,
            tempdir: Some(prepared.tempdir),
            cleaned: false,
        }
    }

    /// The config to resume the session with (store attached).
    pub fn resume_config(&self) -> AgentConfig {
        self.config.clone()
    }

    /// Delete the session from the store and drop the temp directory.
    pub async fn cleanup(&mut self) -> StoreCleanupRecord {
        if self.cleaned {
            return StoreCleanupRecord {
                attempted: true,
                succeeded: true,
                detail: Some("already cleaned".into()),
            };
        }
        let delete_result = self.store.delete(&self.session_id).await;
        let drop_result = self.tempdir.take().map(|td| td.close());

        let mut detail_parts = Vec::new();
        let mut succeeded = true;
        match delete_result {
            Ok(()) => detail_parts.push("store_delete=ok".into()),
            Err(e) => {
                succeeded = false;
                detail_parts.push(format!("store_delete=err:{e}"));
            }
        }
        match drop_result {
            None => detail_parts.push("tempdir=already_taken".into()),
            Some(Ok(())) => detail_parts.push("tempdir=ok".into()),
            Some(Err(e)) => {
                succeeded = false;
                detail_parts.push(format!("tempdir=err:{e}"));
            }
        }
        self.cleaned = succeeded;
        StoreCleanupRecord {
            attempted: true,
            succeeded,
            detail: Some(detail_parts.join("; ")),
        }
    }

    /// Sync cleanup for non-async contexts (best-effort tempdir drop).
    pub fn cleanup_sync(&mut self) -> StoreCleanupRecord {
        if self.cleaned {
            return StoreCleanupRecord {
                attempted: true,
                succeeded: true,
                detail: Some("already cleaned".into()),
            };
        }
        // Best-effort: drop tempdir; store file goes with it.
        let drop_result = self.tempdir.take().map(|td| td.close());
        let (succeeded, detail) = match drop_result {
            None => (true, "tempdir=already_taken".to_string()),
            Some(Ok(())) => (true, "tempdir=ok".to_string()),
            Some(Err(e)) => (false, format!("tempdir=err:{e}")),
        };
        self.cleaned = succeeded;
        StoreCleanupRecord {
            attempted: true,
            succeeded,
            detail: Some(detail),
        }
    }

    #[allow(dead_code)]
    pub fn store_path(&self) -> &Path {
        &self.store_path
    }
}

impl Drop for AttemptSession {
    fn drop(&mut self) {
        // Ensure temp files do not leak if caller forgot cleanup.
        let _ = self.tempdir.take().map(|td| td.close());
    }
}

fn unique_suffix() -> String {
    // Prefer UUID-like uniqueness without adding a uuid crate dep: use RunId.
    RunId::new().to_string()
}

fn block_on_store<F>(fut: F) -> Result<(), ArtifactError>
where
    F: std::future::Future<Output = Result<(), ArtifactError>>,
{
    // Create a tiny current-thread runtime if none is running.
    match tokio::runtime::Handle::try_current() {
        // Must not block in-place on the same runtime; use block_in_place
        // when inside a runtime.
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| ArtifactError::io(format!("tokio runtime: {e}")))?;
            rt.block_on(fut)
        }
    }
}

/// Verify seed bytes normalize stably (hash helper for manifest).
#[allow(dead_code)]
pub fn seed_content_hash(seed: &SessionSeed) -> Result<String, ArtifactError> {
    seed.content_hash()
        .map_err(|e| ArtifactError::new(e.to_string()))
}

/// Expose normalized seed bytes for tests.
#[allow(dead_code)]
pub fn seed_normalized_bytes(seed: &SessionSeed) -> Result<Vec<u8>, ArtifactError> {
    normalize_seed_bytes(seed).map_err(|e| ArtifactError::new(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest::budget::BudgetUsage;
    use orchest::model::{ContentBlock, Message, Role};
    use orchest::run::AgentConfig;

    fn sample_seed(id: &str) -> SessionSeed {
        SessionSeed {
            schema_version: "1".into(),
            seed_id: id.into(),
            messages: vec![
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text("q".into())],
                },
                Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text("a".into())],
                },
            ],
            step: 3,
            budget_used: BudgetUsage {
                tokens_used: 100,
                tool_calls_used: 2,
                cost_usd: 0.0,
            },
        }
    }

    fn base_config() -> AgentConfig {
        AgentConfig::builder("test-agent", "test-model")
            .system_prompt("test")
            .max_steps(5)
            .build()
            .expect("config")
    }

    #[tokio::test]
    async fn materialize_uses_unique_ids_and_independent_stores() {
        let seed = sample_seed("seed-a");
        let mut a = AttemptSession::materialize_async(&seed, base_config())
            .await
            .unwrap();
        let mut b = AttemptSession::materialize_async(&seed, base_config())
            .await
            .unwrap();

        assert_ne!(a.session_id, b.session_id);
        assert_ne!(a.run_id, b.run_id);
        assert_eq!(a.seed_hash, b.seed_hash);
        assert_ne!(a.store_path, b.store_path);

        // Append to store A; B must not see it.
        let next = SessionLog::from_events(a.store.read(&a.session_id, 1).await.unwrap())
            .unwrap()
            .next_seq();
        let mut extra = SessionLog::new();
        for _ in 1..next {
            extra.append(SessionEventKind::StepStarted { step: 0 });
        }
        extra.append(SessionEventKind::StepEnded {
            step: 98,
            budget_used: Default::default(),
        });
        a.store
            .append(&a.session_id, &extra.events()[(next - 1) as usize..])
            .await
            .unwrap();
        let log_a = SessionLog::from_events(a.store.read(&a.session_id, 1).await.unwrap()).unwrap();
        assert_eq!(log_a.step(), 99);

        let log_b = SessionLog::from_events(b.store.read(&b.session_id, 1).await.unwrap()).unwrap();
        assert_eq!(log_b.step(), seed.step);

        // Same seed hash across attempts
        assert_eq!(a.seed_id, seed.seed_id);
        assert_eq!(b.seed_id, seed.seed_id);

        let ca = a.cleanup().await;
        let cb = b.cleanup().await;
        assert!(ca.attempted && ca.succeeded, "{ca:?}");
        assert!(cb.attempted && cb.succeeded, "{cb:?}");
        assert!(!a.store_path.exists() || ca.succeeded);
    }

    #[tokio::test]
    async fn cleanup_recorded_on_success_and_is_idempotent() {
        let seed = sample_seed("seed-b");
        let mut session = AttemptSession::materialize_async(&seed, base_config())
            .await
            .unwrap();
        let first = session.cleanup().await;
        assert!(first.attempted);
        assert!(first.succeeded);
        let second = session.cleanup().await;
        assert!(second.attempted);
        assert!(second.succeeded);
        assert_eq!(second.detail.as_deref(), Some("already cleaned"));
    }

    #[tokio::test]
    async fn materialize_preserves_seed_messages_and_budget() {
        let seed = sample_seed("seed-c");
        let mut session = AttemptSession::materialize_async(&seed, base_config())
            .await
            .unwrap();
        let stored =
            SessionLog::from_events(session.store.read(&session.session_id, 1).await.unwrap())
                .unwrap();
        assert_eq!(stored.messages().len(), 2);
        assert_eq!(stored.step(), 3);
        assert_eq!(stored.projection().budget_used.tokens_used, 100);
        assert_eq!(stored.projection().run_id, Some(session.run_id));
        let meta = session
            .store
            .get_meta(&session.session_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(meta.schema_version, SessionMeta::CURRENT_SCHEMA_VERSION);
        // resume config carries the store
        let resume = session.resume_config();
        assert_eq!(
            resume.session_id.as_deref(),
            Some(session.session_id.as_str())
        );
        let _ = session.cleanup().await;
    }

    #[test]
    fn sync_materialize_and_cleanup() {
        let seed = sample_seed("seed-d");
        let mut session = AttemptSession::materialize(&seed, base_config()).unwrap();
        assert!(session.store_path.exists());
        let rec = session.cleanup_sync();
        assert!(rec.attempted);
        assert!(rec.succeeded);
    }
}
