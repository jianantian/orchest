//! Follow-up session seed materialization with per-attempt isolation.
//!
//! Each attempt materializes a `SessionSnapshot` with unique session/run IDs
//! into its own temporary SQLite store. Cleanup results are recorded for
//! `attempt.json`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use orchest::run::{AgentConfig, RunId};
use orchest::session::{SessionSnapshot, SessionStore, SqliteSessionStore};
use tempfile::TempDir;

use super::artifact::{ArtifactError, StoreCleanupRecord};
use super::case::{normalize_seed_bytes, SessionSeed};

/// Materialized attempt session with cleanup guard.
pub struct AttemptSession {
    pub session_id: String,
    pub run_id: RunId,
    pub seed_id: String,
    pub seed_hash: String,
    pub snapshot: SessionSnapshot,
    pub store: Arc<dyn SessionStore>,
    pub store_path: PathBuf,
    tempdir: Option<TempDir>,
    cleaned: bool,
}

impl AttemptSession {
    /// Materialize a validated seed into a unique snapshot + temp SQLite store.
    pub fn materialize(
        seed: &SessionSeed,
        base_config: AgentConfig,
    ) -> Result<Self, ArtifactError> {
        let seed_hash = seed
            .content_hash()
            .map_err(|e| ArtifactError::new(e.to_string()))?;
        let session_id = format!("eval-{}-{}", seed.seed_id, unique_suffix());
        let run_id = RunId::new();

        let tempdir = TempDir::new()
            .map_err(|e| ArtifactError::io(format!("creating temp session dir: {e}")))?;
        let store_path = tempdir.path().join("session.sqlite3");
        let store = SqliteSessionStore::open(&store_path)
            .map_err(|e| ArtifactError::io(format!("opening temp session store: {e}")))?;
        let store: Arc<dyn SessionStore> = Arc::new(store);

        let active_config = base_config.with_session_store(Arc::clone(&store), session_id.clone());

        let snapshot = SessionSnapshot {
            schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.to_string(),
            session_id: session_id.clone(),
            run_id,
            messages: seed.messages.clone(),
            step: seed.step,
            budget_used: seed.budget_used.clone(),
            active_config,
        };

        // Persist so resume paths can re-load if needed.
        // Use block_on-friendly async: callers in async tests use the async helper.
        // For sync materialize we save via a tiny runtime if needed.
        save_snapshot_blocking(&store, &session_id, &snapshot)?;

        Ok(Self {
            session_id,
            run_id,
            seed_id: seed.seed_id.clone(),
            seed_hash,
            snapshot,
            store,
            store_path,
            tempdir: Some(tempdir),
            cleaned: false,
        })
    }

    /// Async materialize that saves via the store's async API.
    pub async fn materialize_async(
        seed: &SessionSeed,
        base_config: AgentConfig,
    ) -> Result<Self, ArtifactError> {
        let seed_hash = seed
            .content_hash()
            .map_err(|e| ArtifactError::new(e.to_string()))?;
        let session_id = format!("eval-{}-{}", seed.seed_id, unique_suffix());
        let run_id = RunId::new();

        let tempdir = TempDir::new()
            .map_err(|e| ArtifactError::io(format!("creating temp session dir: {e}")))?;
        let store_path = tempdir.path().join("session.sqlite3");
        let store = SqliteSessionStore::open(&store_path)
            .map_err(|e| ArtifactError::io(format!("opening temp session store: {e}")))?;
        let store: Arc<dyn SessionStore> = Arc::new(store);

        let active_config = base_config.with_session_store(Arc::clone(&store), session_id.clone());

        let snapshot = SessionSnapshot {
            schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.to_string(),
            session_id: session_id.clone(),
            run_id,
            messages: seed.messages.clone(),
            step: seed.step,
            budget_used: seed.budget_used.clone(),
            active_config,
        };

        store
            .save(&session_id, &snapshot)
            .await
            .map_err(|e| ArtifactError::io(format!("saving materialized seed: {e}")))?;

        Ok(Self {
            session_id,
            run_id,
            seed_id: seed.seed_id.clone(),
            seed_hash,
            snapshot,
            store,
            store_path,
            tempdir: Some(tempdir),
            cleaned: false,
        })
    }

    /// Detach a clone of the snapshot with store re-attached (for resume).
    pub fn resume_snapshot(&self) -> SessionSnapshot {
        let mut snap = self.snapshot.clone();
        snap.active_config = snap
            .active_config
            .with_session_store(Arc::clone(&self.store), self.session_id.clone());
        snap
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

fn save_snapshot_blocking(
    store: &Arc<dyn SessionStore>,
    session_id: &str,
    snapshot: &SessionSnapshot,
) -> Result<(), ArtifactError> {
    // Create a tiny current-thread runtime if none is running.
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            // Safety: must not block in-place on the same runtime; use spawn_blocking path
            // via block_in_place when inside a runtime.
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    store
                        .save(session_id, snapshot)
                        .await
                        .map_err(|e| ArtifactError::io(format!("saving materialized seed: {e}")))
                })
            })
        }
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| ArtifactError::io(format!("tokio runtime: {e}")))?;
            rt.block_on(async {
                store
                    .save(session_id, snapshot)
                    .await
                    .map_err(|e| ArtifactError::io(format!("saving materialized seed: {e}")))
            })
        }
    }
}

/// Verify seed bytes normalize stably (hash helper for manifest).
pub fn seed_content_hash(seed: &SessionSeed) -> Result<String, ArtifactError> {
    seed.content_hash()
        .map_err(|e| ArtifactError::new(e.to_string()))
}

/// Expose normalized seed bytes for tests.
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
        AgentConfig::builder("test-model")
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

        // Mutate store A; B must not see it.
        let mut snap_a = a.store.load(&a.session_id).await.unwrap().unwrap();
        snap_a.step = 99;
        a.store.save(&a.session_id, &snap_a).await.unwrap();

        let snap_b = b.store.load(&b.session_id).await.unwrap().unwrap();
        assert_eq!(snap_b.step, seed.step);
        assert_ne!(snap_b.step, 99);

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
        assert_eq!(session.snapshot.messages.len(), 2);
        assert_eq!(session.snapshot.step, 3);
        assert_eq!(session.snapshot.budget_used.tokens_used, 100);
        assert_eq!(
            session.snapshot.schema_version,
            SessionSnapshot::CURRENT_SCHEMA_VERSION
        );
        // resume snapshot re-attaches store
        let resume = session.resume_snapshot();
        assert_eq!(resume.session_id, session.session_id);
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
