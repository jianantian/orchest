//! SessionSnapshot: serializable point-in-time capture of an agent run.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: String,
    pub session_id: String,
    pub run_id: crate::run::RunId,
    pub messages: Vec<crate::model::Message>,
    pub step: u32,
    pub budget_used: crate::budget::BudgetUsage,
    /// The active agent config at the time of snapshot (reflects post-handoff agent).
    /// `#[serde(skip)]` fields (hooks, retry_policy, handoffs, session_store) are
    /// empty after deserialization; callers must re-register them before calling
    /// `AgentRun::resume`/`resume_with_input`. In particular, if the snapshot's
    /// `session_id` is set (persistence was enabled) but `session_store` wasn't
    /// re-attached via `with_session_store`, resume fails loudly with
    /// `ConfigError::SessionStoreMissing` rather than silently dropping persistence.
    pub active_config: crate::run::AgentConfig,
}

impl SessionSnapshot {
    pub const CURRENT_SCHEMA_VERSION: &'static str = "0.1";
}
