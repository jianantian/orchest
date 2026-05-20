pub mod async_job;
pub mod builtin;
pub mod in_process;
pub mod registry;

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::run::RunId;
use async_job::JobHandle;

pub type JsonSchema = serde_json::Value;

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
    AsyncJob(JobHandle),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub side_effect: bool,
    pub requires_approval: bool,
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

#[derive(Debug)]
pub struct ToolContext {
    pub run_id: RunId,
    pub tool_call_id: String,
    pub on_update: Option<mpsc::Sender<Value>>,
    pub event_tx: Option<mpsc::Sender<crate::events::RuntimeEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub message: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}
