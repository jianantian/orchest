// Run configuration types: RunId, AgentConfig, RunState, RunStatus, SubAgentRuntime.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::model::{Message, ModelSpec, RequestOptions};
use crate::tool::mcp::McpServerConfig;
use crate::tool::Tool;

use super::helpers::{min_option, min_option_f64};

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
    pub model: ModelConfig,
    pub budget: BudgetConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub spec: ModelSpec,
    #[serde(default)]
    pub options: RequestOptions,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            spec: ModelSpec {
                provider: String::new(),
                model: String::new(),
                api_key_env: None,
                api_url: None,
                max_tokens: None,
                context_window_size: None,
            },
            options: RequestOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    pub dir: Option<String>,
    pub allowed: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_steps: u32,
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub tool_search_enabled: bool,
    #[serde(default)]
    pub compaction: Option<CompactionConfig>,
    #[serde(default)]
    pub webhook_enabled: bool,
    #[serde(default)]
    pub code_execution_enabled: bool,
    #[serde(default)]
    pub run_depth: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_steps: 100,
            allowed_tools: None,
            mcp_servers: vec![],
            tool_search_enabled: false,
            compaction: None,
            webhook_enabled: false,
            code_execution_enabled: false,
            run_depth: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub threshold: f32,
    pub recent_messages: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            threshold: 0.8,
            recent_messages: 10,
        }
    }
}

// Builder

impl AgentConfig {
    pub fn builder(model: impl Into<String>) -> AgentConfigBuilder {
        AgentConfigBuilder::new(model)
    }
}

pub struct AgentConfigBuilder {
    system_prompt: String,
    model: ModelConfig,
    budget: BudgetConfig,
    skills: SkillsConfig,
    runtime: RuntimeConfig,
}

impl AgentConfigBuilder {
    pub fn new(model: impl Into<String>) -> Self {
        let model_str = model.into();
        Self {
            system_prompt: String::new(),
            model: ModelConfig {
                spec: ModelSpec {
                    provider: String::new(),
                    model: model_str,
                    api_key_env: None,
                    api_url: None,
                    max_tokens: None,
                    context_window_size: None,
                },
                options: RequestOptions::default(),
            },
            budget: BudgetConfig {
                max_tokens: None,
                max_tool_calls: None,
                max_duration: None,
                max_cost_usd: None,
            },
            skills: SkillsConfig::default(),
            runtime: RuntimeConfig::default(),
        }
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }
    pub fn max_cost_usd(mut self, usd: f64) -> Self {
        self.budget.max_cost_usd = Some(usd);
        self
    }
    pub fn max_tokens(mut self, n: u64) -> Self {
        self.budget.max_tokens = Some(n);
        self
    }
    pub fn max_tool_calls(mut self, n: u32) -> Self {
        self.budget.max_tool_calls = Some(n);
        self
    }
    pub fn skills_dir(mut self, dir: impl Into<String>) -> Self {
        self.skills.dir = Some(dir.into());
        self
    }
    pub fn allowed_skills(mut self, skills: Vec<String>) -> Self {
        self.skills.allowed = Some(skills);
        self
    }
    pub fn max_steps(mut self, n: u32) -> Self {
        self.runtime.max_steps = n;
        self
    }
    pub fn mcp_server(mut self, config: McpServerConfig) -> Self {
        self.runtime.mcp_servers.push(config);
        self
    }
    pub fn enable_tool_search(mut self) -> Self {
        self.runtime.tool_search_enabled = true;
        self
    }
    pub fn enable_code_execution(mut self) -> Self {
        self.runtime.code_execution_enabled = true;
        self
    }
    pub fn enable_compaction(mut self, config: CompactionConfig) -> Self {
        self.runtime.compaction = Some(config);
        self
    }
    pub fn run_depth(mut self, depth: u32) -> Self {
        self.runtime.run_depth = depth;
        self
    }
    pub fn build(self) -> AgentConfig {
        AgentConfig {
            system_prompt: self.system_prompt,
            model: self.model,
            budget: self.budget,
            skills: self.skills,
            runtime: self.runtime,
        }
    }
}

// Runtime types

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
mod tests {
    use super::*;

    #[test]
    fn builder_sets_fields_correctly() {
        let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
            .system_prompt("test")
            .max_cost_usd(1.0)
            .skills_dir("./skills")
            .max_steps(50)
            .enable_tool_search()
            .build();
        assert_eq!(config.system_prompt, "test");
        assert_eq!(config.budget.max_cost_usd, Some(1.0));
        assert_eq!(config.skills.dir.as_deref(), Some("./skills"));
        assert_eq!(config.runtime.max_steps, 50);
        assert!(config.runtime.tool_search_enabled);
    }

    #[test]
    fn serde_round_trip() {
        let config = AgentConfig::builder("test-model")
            .system_prompt("round trip")
            .max_steps(5)
            .enable_compaction(CompactionConfig::default())
            .build();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: AgentConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.system_prompt, "round trip");
        assert_eq!(deserialized.runtime.max_steps, 5);
        assert!(deserialized.runtime.compaction.is_some());
    }
}
