use std::sync::Arc;
use std::time::Duration;

use agent_runtime_core::budget::BudgetConfig;
use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse, ModelSpec,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use agent_runtime_core::run::{AgentConfig, AgentRun, ModelConfig, RuntimeConfig, SkillsConfig};
use agent_runtime_core::skill::{SkillDependencies, SkillEnvManager, SkillManifest};
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{Tool, ToolError, ToolMetadata, ToolOutput, ToolSource};
use serde_json::json;
use tokio::sync::mpsc;

fn test_config() -> AgentConfig {
    AgentConfig {
        system_prompt: "test".into(),
        model: ModelConfig {
            spec: ModelSpec {
                provider: "test".into(),
                model: "test".into(),
                api_key_env: None,
                api_url: None,
                max_tokens: None,
                context_window_size: None,
            },
            options: RequestOptions::default(),
        },
        budget: BudgetConfig {
            max_tokens: Some(1_000),
            max_tool_calls: Some(20),
            max_duration: Some(Duration::from_secs(30)),
            max_cost_usd: None,
        },
        skills: SkillsConfig::default(),
        runtime: RuntimeConfig {
            max_steps: 4,
            code_execution_enabled: true,
            ..RuntimeConfig::default()
        },
        hooks: vec![],
        retry_policy: None,
    }
}

struct CodeExecModel;

#[async_trait::async_trait]
impl ModelAdapter for CodeExecModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "mock"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[agent_runtime_core::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            ..Default::default()
        };
        let tool_results = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter(|block| matches!(block, ContentBlock::ToolResult { .. }))
            .count();
        if tool_results == 0 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "one".into(),
                    name: "execute_python".into(),
                    input: json!({"code": "x = 41\nprint(x)", "timeout_seconds": 5}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else if tool_results == 1 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "two".into(),
                    name: "execute_python".into(),
                    input: json!({"code": "print(x + 1)", "timeout_seconds": 5}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

struct SubAgentModel;

#[async_trait::async_trait]
impl ModelAdapter for SubAgentModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "mock"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[agent_runtime_core::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 2,
            output_tokens: 3,
            ..Default::default()
        };
        let has_child_task = messages.iter().any(|message| {
            message
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::Text(text) if text == "child task"))
        });
        let has_tool_result = messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|block| matches!(block, ContentBlock::ToolResult { .. }));

        if has_child_task {
            return Ok(ModelResponse {
                content: vec![ContentBlock::Text("child done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            });
        }
        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("parent done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "sub".into(),
                    name: "spawn_child".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

fn make_spawn_child_tool() -> Arc<dyn Tool> {
    let mut config = test_config();
    config.budget.max_tokens = Some(20);
    config.budget.max_tool_calls = Some(3);
    config.budget.max_duration = Some(Duration::from_secs(5));
    config.as_tool(
        "spawn_child",
        "requests a sub-agent",
        Arc::new(SubAgentModel),
        ToolRegistry::new(),
        Arc::new(|_| Ok("child task".into())),
        Arc::new(|details| details.get("output").cloned().unwrap_or(details.clone())),
    )
}

#[test]
fn skill_env_manager_uses_hashed_python_cache_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = SkillEnvManager::new(tmp.path().to_path_buf());
    let manifest = SkillManifest {
        name: "demo".into(),
        description: "demo".into(),
        path: tmp.path().join("demo"),
        allowed_tools: None,
        bundled_tools: vec![],
        dependencies: SkillDependencies {
            python: vec!["b==2".into(), "a==1".into()],
            node: Default::default(),
        },
        capabilities: None,
        raw_frontmatter: json!({}),
    };

    let path = manager.python_env_path(&manifest);

    assert!(path.starts_with(tmp.path().join("skill-envs")));
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("demo-"));
    assert_eq!(
        path.file_name().unwrap().to_string_lossy().len(),
        "demo-".len() + 8
    );
}

#[tokio::test]
async fn code_execution_registers_tools_and_reuses_python_session() {
    let (handle, mut rx) = AgentRun::start(
        test_config(),
        "run code".into(),
        Arc::new(CodeExecModel),
        ToolRegistry::new(),
    );

    let mut outputs = Vec::new();
    let mut updates = Vec::new();
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ToolCallUpdate { tool, partial, .. } if tool == "execute_python" => {
                updates.push(partial);
            }
            RuntimeEvent::ToolCallCompleted { tool, output, .. } if tool == "execute_python" => {
                outputs.push(output);
            }
            _ => {}
        }
    }
    handle.wait().await;

    assert_eq!(outputs.len(), 2);
    assert!(outputs[0]["stdout"].as_str().unwrap().contains("41"));
    assert!(outputs[1]["stdout"].as_str().unwrap().contains("42"));
    assert!(updates.iter().any(|value| value["stdout_line"] == "41"));
    assert!(updates.iter().any(|value| value["stdout_line"] == "42"));
}

#[test]
fn sub_agent_budget_is_capped_by_parent_remaining() {
    let requested = BudgetConfig {
        max_tokens: Some(100),
        max_tool_calls: Some(10),
        max_duration: Some(Duration::from_secs(120)),
        max_cost_usd: Some(5.0),
    };
    let parent_remaining = BudgetConfig {
        max_tokens: Some(40),
        max_tool_calls: Some(3),
        max_duration: Some(Duration::from_secs(10)),
        max_cost_usd: Some(1.0),
    };

    let capped =
        agent_runtime_core::run::SubAgentRuntime::cap_budget(&requested, &parent_remaining);

    assert_eq!(capped.max_tokens, Some(40));
    assert_eq!(capped.max_tool_calls, Some(3));
    assert_eq!(capped.max_duration, Some(Duration::from_secs(10)));
    assert_eq!(capped.max_cost_usd, Some(1.0));
}

#[tokio::test]
async fn sub_agent_request_forwards_events_and_completes_parent_tool_result() {
    let mut registry = ToolRegistry::new();
    registry.register(make_spawn_child_tool()).unwrap();
    let (handle, mut rx) = AgentRun::start(
        test_config(),
        "parent task".into(),
        Arc::new(SubAgentModel),
        registry,
    );

    let mut saw_started = false;
    let mut saw_child_completion = false;
    let mut saw_lifecycle_completion = false;
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::SubAgentStarted {
                parent_run_id,
                child_run_id,
                ..
            } => {
                saw_started = true;
                assert_ne!(parent_run_id, child_run_id);
            }
            RuntimeEvent::SubAgentEvent { event, .. } if matches!(event.as_ref(), RuntimeEvent::RunCompleted { output } if output == "child done") =>
            {
                saw_child_completion = true;
            }
            RuntimeEvent::SubAgentCompleted {
                output,
                budget_used,
                ..
            } => {
                saw_lifecycle_completion = true;
                assert_eq!(output, "child done");
                assert_eq!(budget_used.tokens_used, 5);
            }
            _ => {}
        }
    }
    handle.wait().await;

    assert!(saw_started);
    assert!(saw_child_completion);
    assert!(saw_lifecycle_completion);
}

#[tokio::test]
async fn agent_tool_runs_child_agent_with_isolated_context() {
    let mut registry = ToolRegistry::new();
    registry
        .register(test_config().as_tool(
            "spawn_child",
            "delegates to a child agent",
            Arc::new(SubAgentModel),
            ToolRegistry::new(),
            Arc::new(|_| Ok("child task".into())),
            Arc::new(|details| details.get("output").cloned().unwrap_or(details.clone())),
        ))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(
        test_config(),
        "parent task".into(),
        Arc::new(SubAgentModel),
        registry,
    );

    let mut saw_child_done = false;
    let mut saw_parent_tool_details = false;
    let mut final_output = None;
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::SubAgentEvent { event, .. } if matches!(event.as_ref(), RuntimeEvent::RunCompleted { output } if output == "child done") =>
            {
                saw_child_done = true;
            }
            RuntimeEvent::ToolCallCompleted { tool, output, .. } if tool == "spawn_child" => {
                saw_parent_tool_details = true;
                assert_eq!(output["output"], "child done");
                assert!(output.get("child_run_id").is_some());
            }
            RuntimeEvent::RunCompleted { output } => {
                final_output = Some(output);
            }
            _ => {}
        }
    }
    handle.wait().await;

    assert!(saw_child_done);
    assert!(saw_parent_tool_details);
    assert_eq!(final_output, Some(json!("parent done")));
}
