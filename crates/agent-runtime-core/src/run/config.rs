// Run configuration types: RunId, AgentConfig, RunState, RunStatus, SubAgentRuntime.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::model::{Message, ModelSpec, RequestOptions};
use crate::tool::mcp::McpServerConfig;
use crate::tool::Tool;

use super::helpers::min_option;
use super::helpers::min_option_f64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub uuid::Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: String,
    pub model: ModelSpec,
    #[serde(default)]
    pub request_options: RequestOptions,
    pub budget: BudgetConfig,
    pub max_steps: u32,
    pub allowed_skills: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub tool_search_enabled: bool,
    #[serde(default)]
    pub compaction_threshold: Option<f32>,
    #[serde(default = "default_recent_messages")]
    pub compaction_recent_messages: usize,
    #[serde(default)]
    pub webhook_enabled: bool,
    #[serde(default)]
    pub code_execution_enabled: bool,
    #[serde(default)]
    pub skills_dir: Option<String>,
    #[serde(default)]
    pub run_depth: u32,
}

fn default_recent_messages() -> usize {
    10
}

#[derive(Serialize, Deserialize)]
pub struct RunState {
    pub run_id: RunId,
    pub schema_version: String,
    pub config: AgentConfig,
    pub messages: Vec<Message>,
    #[serde(skip)]
    pub available_tools: Vec<Arc<dyn Tool>>,
    pub step: u32,
    pub status: RunStatus,
    pub budget_used: BudgetUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RunStatus {
    Running,
    WaitingForApproval {
        tool_call: crate::tool::ToolCall,
    },
    WaitingForAsyncTool {
        tool_call: crate::tool::ToolCall,
        job_handle: crate::tool::async_job::JobHandle,
        #[serde(skip, default = "Instant::now")]
        since: Instant,
    },
    Completed {
        output: Value,
    },
    Failed {
        error: String,
    },
    Aborted,
}

pub struct AgentRun;

pub struct SubAgentRuntime;

impl SubAgentRuntime {
    pub fn cap_budget(requested: &BudgetConfig, parent_remaining: &BudgetConfig) -> BudgetConfig {
        BudgetConfig {
            max_tokens: min_option(requested.max_tokens, parent_remaining.max_tokens),
            max_tool_calls: min_option(requested.max_tool_calls, parent_remaining.max_tool_calls),
            max_duration: min_option(requested.max_duration, parent_remaining.max_duration),
            max_cost_usd: min_option_f64(requested.max_cost_usd, parent_remaining.max_cost_usd),
        }
    }
}

#[cfg(test)]
pub(crate) fn default_recent_messages_value() -> usize {
    default_recent_messages()
}
