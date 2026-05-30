//! Hook framework: extensible lifecycle callbacks for agent runs.

use std::sync::Arc;

use async_trait::async_trait;

pub enum HookAction {
    Continue,
    Skip,
    Abort(String),
}

pub enum ModelHookAction {
    Continue,
    Abort(String),
}

pub struct RunHookContext {
    pub run_id: crate::run::RunId,
    pub agent_name: String,
    pub step: u32,
}

pub struct ModelHookContext {
    pub run_id: crate::run::RunId,
    pub messages: Vec<crate::model::Message>,
    pub model_spec: crate::model::ModelSpec,
}

pub struct ToolHookContext {
    pub run_id: crate::run::RunId,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_metadata: crate::tool::ToolMetadata,
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

pub(crate) mod runner;

/// Helper: build a typed AgentRef wrapper from a raw arc.
pub fn with_hook(hooks: &mut Vec<Arc<dyn Hook>>, hook: Arc<dyn Hook>) {
    hooks.push(hook);
}
