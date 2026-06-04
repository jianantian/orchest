//! Tool trait, types, and submodules for all tool implementations.

pub mod agent_as_tool;
pub mod async_job;
pub mod builtin;
pub mod code_exec;
pub mod error;
pub mod handoff_tool;
pub mod in_process;
pub mod mcp;
pub mod registry;
pub mod search;

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use async_job::JobHandle;

pub use crate::model::{JsonSchema, ToolDef};
pub use error::{ErrorKind, RetryHint, ToolError};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> &JsonSchema;
    fn output_schema(&self) -> Option<&JsonSchema>;
    fn metadata(&self) -> &ToolMetadata;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub side_effect: bool,
    pub approval: Approval,
    pub cost_hint: Option<CostHint>,
    pub timeout: Option<Duration>,
    pub max_output_tokens: Option<u64>,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSource {
    InProcess,
    McpServer { server_id: String },
    Skill { skill_name: String },
    Builtin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CostHint {
    Free,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub run_id: crate::run::RunId,
    pub run_depth: u32,
    pub tool_call_id: String,
    pub on_update: Option<mpsc::Sender<Value>>,
    pub event_tx: Option<mpsc::Sender<crate::events::RuntimeEvent>>,
    pub webhook_base_url: Option<String>,
    /// Shared approval bus for the entire run tree; used by AgentAsTool to forward
    /// child approval requests to the parent's RunHandle.
    pub approval_bus: crate::run::handle::ApprovalBus,
    /// Remaining budget in the parent run at the time this tool is called.
    /// AgentAsTool uses this to cap the child run so it cannot exceed what
    /// the parent has left.
    pub remaining_budget: crate::budget::BudgetConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}
