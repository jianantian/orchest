//! Tool trait, types, and submodules for all tool implementations.

pub mod agent_as_tool;
pub mod async_job;
pub mod builtin;
pub mod code_exec;
pub mod error;
pub mod handoff_tool;
pub mod in_process;
pub mod mcp;
pub mod metadata;
pub mod registry;
pub mod search;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::model::Message;
use async_job::JobHandle;

pub use crate::model::{JsonSchema, ToolDef};
pub use agent_as_tool::ContextMode;
pub use error::{ErrorKind, RetryHint, ToolError};
pub use metadata::{CostHint, ToolExecutionMode, ToolMetadata, ToolParallelism, ToolSource};

/// A capability the model can invoke: name, schemas, metadata, and an async
/// `execute`. Implement this trait (or use `InProcessTool`) and register the
/// `Arc<dyn Tool>` into a [`ToolRegistry`](registry::ToolRegistry).
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> &JsonSchema;
    fn output_schema(&self) -> Option<&JsonSchema>;
    fn metadata(&self) -> &ToolMetadata;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
    /// Execute this tool once with a throwaway [`ToolContext::oneshot()`]
    /// context. This is the zero-boilerplate entry point for calling a tool
    /// outside any run — and for tests — when the caller has no run state to
    /// thread through. Inside a run the runtime builds the real context and
    /// calls [`Tool::execute`] directly; do not use this there.
    async fn call_oneshot(&self, input: Value) -> Result<ToolOutput, ToolError> {
        self.execute(input, &ToolContext::oneshot()).await
    }
    fn needs_parent_context(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub enum ToolOutput {
    Immediate(Value),
    /// `model_output` is what the model sees as the tool result.
    /// `details` is the full structured payload forwarded to events.
    /// `external_usage` carries child-run budget consumed by this call so
    /// the parent `BudgetGuard` can account for it.
    Structured {
        model_output: Value,
        details: Value,
        external_usage: Option<crate::budget::BudgetUsage>,
    },
    AsyncJob(JobHandle),
    Handoff(Box<crate::handoff::HandoffResult>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum Approval {
    Never,
    #[default]
    WhenRisky,
    Always,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub run_id: crate::run::RunId,
    pub run_depth: u32,
    pub tool_call_id: String,
    pub event_tx: Option<mpsc::Sender<crate::events::RuntimeEvent>>,
    pub webhook_base_url: Option<String>,
    /// Shared approval bus for the entire run tree; used by AgentAsTool to forward
    /// child approval requests to the parent's RunHandle.
    pub approval_bus: crate::run::handle::ApprovalBus,
    /// Shared delegated-child registry for the run tree; AgentAsTool registers
    /// each child so the supervisor [`crate::run::RunHandle`] can resolve it.
    pub child_registry: crate::run::handle::ChildRunRegistry,
    /// Remaining budget in the parent run at the time this tool is called.
    /// AgentAsTool uses this to cap the child run so it cannot exceed what
    /// the parent has left.
    pub remaining_budget: crate::budget::BudgetConfig,
    /// Parent run's message history; supplied when AgentAsTool uses
    /// `ContextMode::Fork`.
    pub parent_messages: Vec<Message>,
    /// Snapshot of all event subscribers at tool-call construction time.
    /// Index 0 is the primary event receiver; remaining entries are attached
    /// watcher channels. Used by [`ToolContext::emit_event`] to fan out
    /// tool-originated events (including forwarded child lifecycle/runtime
    /// events) without duplicating delivery on the primary receiver.
    ///
    /// Empty when only [`Self::event_tx`] is set (oneshot / unit tests):
    /// [`Self::emit_event`] then falls back to `event_tx` alone.
    pub event_subs: Vec<crate::events::EventSink>,
}

impl ToolContext {
    /// Build a throwaway context with trivial values for a one-shot tool call
    /// outside any run: a fresh [`RunId`](crate::run::RunId), `run_depth` 0, a
    /// synthesized unique `tool_call_id`, no event channel, no webhook base
    /// URL, a default [`ApprovalBus`](crate::run::ApprovalBus), the default
    /// budget, and no parent messages.
    ///
    /// Intended for driving a tool's `execute` directly from code that is not
    /// inside a run (e.g. an application calling a subagent tool once) and for
    /// tests — callers no longer need to know which fields are safe to fake.
    /// Inside a run the runtime constructs the real context; do not use this
    /// there. When a call site needs one real field (typically an event
    /// channel), override it with struct-update syntax:
    /// `ToolContext { event_tx: Some(tx), ..ToolContext::oneshot() }`.
    ///
    /// [`Tool::call_oneshot`] wraps this for the common case of no overrides.
    pub fn oneshot() -> Self {
        let run_id = crate::run::RunId::new();
        Self {
            run_id,
            run_depth: 0,
            tool_call_id: format!("oneshot-{run_id}"),
            event_tx: None,
            webhook_base_url: None,
            approval_bus: crate::run::ApprovalBus::default(),
            child_registry: crate::run::ChildRunRegistry::default(),
            remaining_budget: crate::budget::BudgetConfig::default(),
            parent_messages: vec![],
            event_subs: vec![],
        }
    }

    /// Emit a runtime event through the documented subscriber contract.
    ///
    /// When [`Self::event_subs`] is non-empty (the normal in-run path), the
    /// event is delivered once to the primary receiver and to every attached
    /// watcher subscription known when this tool call started — the same
    /// fan-out the run actor uses for its own events. When `event_subs` is
    /// empty, falls back to a single send on [`Self::event_tx`] for oneshot
    /// and test contexts that only wire a primary channel.
    pub async fn emit_event(&self, event: crate::events::RuntimeEvent) {
        if !self.event_subs.is_empty() {
            crate::events::deliver_to_subscribers(&self.event_subs, event).await;
        } else if let Some(tx) = &self.event_tx {
            let _ = tx.send(event).await;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[cfg(test)]
mod tests;
