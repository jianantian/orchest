//! Hook framework: extensible lifecycle callbacks for agent runs.

use std::sync::Arc;

use async_trait::async_trait;

pub enum HookAction {
    Continue,
    Skip,
    Abort(String),
    /// Reject the current tool call, returning `reason` to the model as the tool
    /// result content. Only meaningful in `before_tool`; treated as `Skip` (with a
    /// warning) if returned from `after_tool` or `before_compact`.
    Reject(String),
}

pub enum ModelHookAction {
    Continue,
    Abort(String),
}

pub struct RunHookContext {
    pub run_id: crate::run::RunId,
    pub agent_name: String,
    pub step: u32,
    /// Budget consumed so far. Meaningful only at `on_run_end` / `on_run_error`
    /// (zero-valued at `on_run_start`).
    pub budget_used: crate::budget::BudgetUsage,
    /// Final conversation history. Populated at `on_run_end` / `on_run_error`.
    pub final_messages: Vec<crate::model::Message>,
    /// The currently active agent config (reflects the post-handoff agent).
    /// `None` until populated at run termination.
    pub active_config: Option<crate::run::AgentConfig>,
}

pub struct ModelHookContext {
    pub run_id: crate::run::RunId,
    pub messages: Vec<crate::model::Message>,
    pub model_spec: crate::model::ModelSpec,
    /// Model response content. `None` in `before_model`; `Some(output)` in
    /// `after_model`. Mutating it in `after_model` rewrites what is appended to
    /// the durable conversation history.
    pub response: Option<Vec<crate::model::ContentBlock>>,
}

pub struct ToolHookContext {
    pub run_id: crate::run::RunId,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_metadata: crate::tool::ToolMetadata,
    /// Tool output. `None` in `before_tool`; `Some(output)` in `after_tool`.
    /// Mutating it in `after_tool` rewrites the tool result content seen by the model.
    pub tool_output: Option<serde_json::Value>,
}

pub struct HandoffHookContext {
    pub run_id: crate::run::RunId,
    pub previous_agent: String,
    pub new_agent: String,
    pub handoff_input: serde_json::Value,
}

pub struct CompactHookContext {
    pub run_id: crate::run::RunId,
    pub messages: Vec<crate::model::Message>,
    pub token_count: u32,
}

/// Lifecycle hook for agent runs. All methods have no-op defaults.
///
/// Implementations must be `Send + Sync`. Panics inside hook methods are caught
/// by the runner and emitted as `HookPanicked` events — the run continues.
#[async_trait]
pub trait Hook: Send + Sync {
    fn persistence_session_id(&self) -> Option<&str> {
        None
    }

    async fn on_run_start(&self, _ctx: &mut RunHookContext) {}
    async fn on_run_end(&self, _ctx: &RunHookContext) {}
    async fn on_run_error(&self, _ctx: &RunHookContext, _error: &str) {}

    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        ModelHookAction::Continue
    }
    async fn after_model(&self, _ctx: &mut ModelHookContext) -> HookAction {
        HookAction::Continue
    }

    async fn before_tool(&self, _ctx: &mut ToolHookContext) -> HookAction {
        HookAction::Continue
    }
    async fn after_tool(&self, _ctx: &mut ToolHookContext) -> HookAction {
        HookAction::Continue
    }

    async fn on_handoff(&self, _ctx: &HandoffHookContext) {}

    async fn before_compact(&self, _ctx: &mut CompactHookContext) -> HookAction {
        HookAction::Continue
    }
}

pub mod loop_detection;
pub(crate) mod runner;

pub use loop_detection::{LoopDetectionConfig, LoopDetectionHook};

/// Helper: append a lifecycle hook to a hook list.
pub fn with_hook(hooks: &mut Vec<Arc<dyn Hook>>, hook: Arc<dyn Hook>) {
    hooks.push(hook);
}
