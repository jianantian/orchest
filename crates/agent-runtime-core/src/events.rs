use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::model::{ModelStreamChunk, TokenUsage};
use crate::run::RunId;
use crate::tool::async_job::JobStatus;
use crate::tool::{ToolCall, ToolSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    RunStarted {
        run_id: RunId,
    },

    ModelCallStarted {
        step: u32,
    },
    ModelStreamChunk {
        delta: ModelStreamChunk,
    },
    ModelCallCompleted {
        tokens: TokenUsage,
    },

    ToolCallStarted {
        tool: String,
        source: ToolSource,
        input: Value,
    },
    ToolCallUpdate {
        tool: String,
        tool_call_id: String,
        partial: Value,
    },
    ToolCallCompleted {
        tool: String,
        output: Value,
        duration: Duration,
    },
    ToolCallFailed {
        tool: String,
        error: String,
    },

    AsyncToolStarted {
        tool: String,
        job_id: String,
    },
    AsyncToolProgress {
        tool: String,
        job_id: String,
        status: JobStatus,
    },
    AsyncToolCompleted {
        tool: String,
        job_id: String,
        output: Value,
        elapsed: Duration,
    },

    SkillContentRead {
        skill_name: String,
        file: String,
        tokens: u32,
    },

    ApprovalRequested {
        tool_call: ToolCall,
    },
    ApprovalGranted {
        tool_call: ToolCall,
    },
    ApprovalDenied {
        tool_call: ToolCall,
    },

    BudgetWarning {
        used: BudgetUsage,
        limit: BudgetConfig,
    },

    RunCompleted {
        output: Value,
    },
    RunFailed {
        error: String,
    },
}
