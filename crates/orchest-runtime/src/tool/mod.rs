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
    /// Remaining budget in the parent run at the time this tool is called.
    /// AgentAsTool uses this to cap the child run so it cannot exceed what
    /// the parent has left.
    pub remaining_budget: crate::budget::BudgetConfig,
    /// Parent run's message history; supplied when AgentAsTool uses
    /// `ContextMode::Fork`.
    pub parent_messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}
