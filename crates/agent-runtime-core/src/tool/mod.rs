pub mod agent;
pub mod async_job;
pub mod builtin;
pub mod code_exec;
pub mod in_process;
pub mod mcp;
pub mod registry;
pub mod search;

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::model::ModelAdapter;
use crate::run::AgentConfig;
use crate::run::RunId;
use crate::tool::registry::ToolRegistry;
use async_job::JobHandle;

pub use crate::model::{JsonSchema, ToolDef};

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
    Structured { model_output: Value, details: Value },
    AgentDelegate(Box<AgentDelegate>),
    AsyncJob(JobHandle),
}

#[derive(Clone)]
pub struct AgentDelegate {
    pub input: String,
    pub config: AgentConfig,
    pub model: std::sync::Arc<dyn ModelAdapter>,
    pub registry: ToolRegistry,
    pub output_mapper: std::sync::Arc<dyn Fn(Value) -> Value + Send + Sync>,
}

impl std::fmt::Debug for AgentDelegate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDelegate")
            .field("input", &self.input)
            .field("config", &self.config)
            .field("registry", &"<tool registry>")
            .field("output_mapper", &"<output mapper>")
            .finish()
    }
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
    pub run_depth: u32,
    pub tool_call_id: String,
    pub on_update: Option<mpsc::Sender<Value>>,
    pub event_tx: Option<mpsc::Sender<crate::events::RuntimeEvent>>,
    pub webhook_base_url: Option<String>,
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
