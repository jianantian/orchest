//! SessionPersistenceHook: auto-saves a snapshot at run termination.

use std::sync::Arc;

use async_trait::async_trait;

use crate::hook::{Hook, RunHookContext};
use crate::run::AgentConfig;

use super::snapshot::SessionSnapshot;
use super::store::SessionStore;

pub struct SessionPersistenceHook {
    pub store: Arc<dyn SessionStore>,
    pub session_id: String,
    /// The config at the time the hook was registered (used when no handoff occurred).
    original_config: AgentConfig,
}

impl SessionPersistenceHook {
    pub fn new(
        store: Arc<dyn SessionStore>,
        session_id: String,
        original_config: AgentConfig,
    ) -> Self {
        Self {
            store,
            session_id,
            original_config,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    fn build_snapshot(&self, ctx: &RunHookContext) -> SessionSnapshot {
        SessionSnapshot {
            schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
            session_id: self.session_id.clone(),
            run_id: ctx.run_id,
            messages: ctx.final_messages.clone(),
            step: ctx.step,
            budget_used: ctx.budget_used.clone(),
            active_config: ctx
                .active_config
                .clone()
                .unwrap_or_else(|| self.original_config.clone()),
        }
    }
}

#[async_trait]
impl Hook for SessionPersistenceHook {
    fn persistence_session_id(&self) -> Option<&str> {
        Some(self.session_id())
    }

    async fn on_run_end(&self, ctx: &RunHookContext) {
        let snap = self.build_snapshot(ctx);
        if let Err(e) = self.store.save(&self.session_id, &snap).await {
            tracing::error!("SessionPersistenceHook: save failed: {e}");
        }
    }

    async fn on_run_error(&self, ctx: &RunHookContext, _error: &str) {
        let snap = self.build_snapshot(ctx);
        if let Err(e) = self.store.save(&self.session_id, &snap).await {
            tracing::error!("SessionPersistenceHook: save on error failed: {e}");
        }
    }
}
