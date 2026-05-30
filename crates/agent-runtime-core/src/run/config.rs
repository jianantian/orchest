//! Run configuration types: RunId, AgentConfig, RunState, RunStatus, SubAgentRuntime.

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

#[derive(Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: String,
    pub model: ModelConfig,
    pub budget: BudgetConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(skip)]
    pub hooks: Vec<std::sync::Arc<dyn crate::hook::Hook>>,
    #[serde(skip)]
    pub retry_policy: Option<super::retry::RetryPolicy>,
    #[serde(skip)]
    pub handoffs: Vec<crate::handoff::Handoff>,
}

impl std::fmt::Debug for AgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentConfig")
            .field("system_prompt", &self.system_prompt)
            .field("model", &self.model)
            .field("budget", &self.budget)
            .field("skills", &self.skills)
            .field("runtime", &self.runtime)
            .field("handoffs", &self.handoffs.len())
            .finish_non_exhaustive()
    }
}

impl AgentConfig {
    /// Wraps this config as a `Tool` that runs a child agent when called.
    ///
    /// - `input_mapper` converts the tool's JSON input to the child agent's prompt string.
    /// - `output_extractor` converts the child's result `Value` to the tool's return value.
    #[allow(clippy::too_many_arguments)] // justified: all parameters are required to instantiate AgentAsTool; a builder is planned for v0.8
    pub fn as_tool(
        &self,
        name: &str,
        description: &str,
        model: std::sync::Arc<dyn crate::model::ModelAdapter>,
        registry: crate::tool::registry::ToolRegistry,
        input_mapper: std::sync::Arc<
            dyn Fn(serde_json::Value) -> Result<String, crate::tool::ToolError> + Send + Sync,
        >,
        output_extractor: std::sync::Arc<
            dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync,
        >,
    ) -> std::sync::Arc<dyn crate::tool::Tool> {
        std::sync::Arc::new(crate::tool::agent_as_tool::AgentAsTool::new(
            self.clone(),
            name.to_string(),
            description.to_string(),
            serde_json::json!({"type": "object", "properties": {"input": {"type": "string"}}}),
            model,
            registry,
            input_mapper,
            output_extractor,
        ))
    }
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
    /// Whether Python/JavaScript code execution is enabled for this agent.
    ///
    /// **Security note**: Code runs in a bare subprocess without sandboxing.
    /// Do not enable for untrusted user input without additional isolation
    /// (e.g., containers, `nsjail`, or a remote execution backend).
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

    pub fn with_hook(mut self, hook: std::sync::Arc<dyn crate::hook::Hook>) -> Self {
        self.hooks.push(hook);
        self
    }

    pub fn with_handoff(mut self, handoff: crate::handoff::Handoff) -> Self {
        self.handoffs.push(handoff);
        self
    }

    pub fn with_loop_detection(self) -> Self {
        self.with_hook(std::sync::Arc::new(
            crate::hook::LoopDetectionHook::default(),
        ))
    }

    pub fn with_loop_detection_config(self, config: crate::hook::LoopDetectionConfig) -> Self {
        self.with_hook(std::sync::Arc::new(crate::hook::LoopDetectionHook::from(
            config,
        )))
    }
}

pub struct AgentConfigBuilder {
    system_prompt: String,
    model: ModelConfig,
    budget: BudgetConfig,
    skills: SkillsConfig,
    runtime: RuntimeConfig,
    hooks: Vec<std::sync::Arc<dyn crate::hook::Hook>>,
    retry_policy: Option<super::retry::RetryPolicy>,
    handoffs: Vec<crate::handoff::Handoff>,
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
            hooks: vec![],
            retry_policy: None,
            handoffs: vec![],
        }
    }

    pub fn retry_policy(mut self, policy: super::retry::RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
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
    pub fn build(self) -> Result<AgentConfig, ConfigError> {
        if self.model.spec.model.is_empty() {
            return Err(ConfigError::MissingModel);
        }
        if self.runtime.max_steps == 0 {
            return Err(ConfigError::InvalidMaxSteps(0));
        }
        if let Some(cost) = self.budget.max_cost_usd {
            if cost < 0.0 {
                return Err(ConfigError::InvalidMaxCost(cost));
            }
        }
        if let Some(tokens) = self.budget.max_tokens {
            if tokens == 0 {
                return Err(ConfigError::InvalidMaxTokens(0));
            }
        }
        if let Some(calls) = self.budget.max_tool_calls {
            if calls == 0 {
                return Err(ConfigError::InvalidMaxToolCalls(0));
            }
        }
        Ok(AgentConfig {
            system_prompt: self.system_prompt,
            model: self.model,
            budget: self.budget,
            skills: self.skills,
            runtime: self.runtime,
            hooks: self.hooks,
            retry_policy: self.retry_policy,
            handoffs: self.handoffs,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("model must be specified")]
    MissingModel,
    #[error("max_steps must be > 0, got {0}")]
    InvalidMaxSteps(u32),
    #[error("max_cost_usd must be non-negative, got {0}")]
    InvalidMaxCost(f64),
    #[error("max_tokens must be > 0, got {0}")]
    InvalidMaxTokens(u64),
    #[error("max_tool_calls must be > 0, got {0}")]
    InvalidMaxToolCalls(u32),
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
            .build()
            .unwrap();
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
            .build()
            .unwrap();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: AgentConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.system_prompt, "round trip");
        assert_eq!(deserialized.runtime.max_steps, 5);
        assert!(deserialized.runtime.compaction.is_some());
    }

    #[test]
    fn missing_model_rejected() {
        let err = AgentConfig::builder("").build().unwrap_err();
        assert!(matches!(err, ConfigError::MissingModel));
    }

    #[test]
    fn zero_max_steps_rejected() {
        let err = AgentConfig::builder("m").max_steps(0).build().unwrap_err();
        assert!(matches!(err, ConfigError::InvalidMaxSteps(0)));
    }

    #[test]
    fn negative_max_cost_rejected() {
        let err = AgentConfig::builder("m")
            .max_cost_usd(-1.0)
            .build()
            .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidMaxCost(c) if c < 0.0));
    }

    #[test]
    fn zero_max_tokens_rejected() {
        let _no_max = AgentConfig::builder("m").build().unwrap(); // default has no max_tokens
                                                                  // Now test with explicit 0
        let mut builder = AgentConfig::builder("m");
        builder.budget.max_tokens = Some(0);
        let err = builder.build().unwrap_err();
        assert!(matches!(err, ConfigError::InvalidMaxTokens(0)));
    }

    #[test]
    fn zero_max_tool_calls_rejected() {
        let mut builder = AgentConfig::builder("m");
        builder.budget.max_tool_calls = Some(0);
        let err = builder.build().unwrap_err();
        assert!(matches!(err, ConfigError::InvalidMaxToolCalls(0)));
    }

    #[test]
    fn valid_config_succeeds() {
        let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
            .system_prompt("test")
            .max_cost_usd(1.0)
            .max_steps(10)
            .build();
        assert!(config.is_ok());
    }
}
