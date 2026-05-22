use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use agent_runtime_core::budget::BudgetConfig;
use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::{
    ContentBlock, Message, ModelAdapter, ModelError, ModelResponse, ModelSpec, ModelStreamChunk,
    StopReason, TokenUsage,
};
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::async_job::{JobHandle, JobStatus};
use agent_runtime_core::tool::builtin::ReadFileTool;
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    JsonSchema, Tool, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

fn test_config() -> AgentConfig {
    AgentConfig {
        system_prompt: "You are a test assistant.".into(),
        model: ModelSpec {
            provider: "test".into(),
            model: "test-model".into(),
            api_key_env: None,
            api_url: None,
            max_tokens: Some(100),
            context_window_size: None,
        },
        budget: BudgetConfig {
            max_tokens: Some(100_000),
            max_tool_calls: Some(10),
            max_duration: Some(Duration::from_secs(30)),
            max_cost_usd: Some(1.0),
        },
        max_steps: 10,
        allowed_skills: None,
        allowed_tools: None,
        mcp_servers: vec![],
        tool_search_enabled: false,
        compaction_threshold: None,
        compaction_recent_messages: 10,
        webhook_enabled: false,
        code_execution_enabled: false,
        skills_dir: None,
        run_depth: 0,
    }
}

struct E2EModelAdapter;

#[async_trait::async_trait]
impl ModelAdapter for E2EModelAdapter {
    async fn stream(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
        };

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if has_tool_result {
            let _ = tx
                .send(ModelStreamChunk::Text {
                    delta: "Task ".into(),
                })
                .await;
            let _ = tx
                .send(ModelStreamChunk::Text {
                    delta: "complete.".into(),
                })
                .await;
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Task complete.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
            })
        } else {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "echo".into(),
                    input: json!({"text": "hello"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
            })
        }
    }
}

struct EchoTool;

#[async_trait::async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "echoes input"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: false,
            requires_approval: false,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(input))
    }
}

struct ThinkingBoundaryModel;

#[async_trait::async_trait]
impl ModelAdapter for ThinkingBoundaryModel {
    async fn stream(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 4,
            output_tokens: 8,
        };
        let _ = tx.send(ModelStreamChunk::ThinkingStart).await;
        let _ = tx
            .send(ModelStreamChunk::Thinking {
                delta: "checking".into(),
            })
            .await;
        let _ = tx.send(ModelStreamChunk::ThinkingEnd).await;
        let _ = tx
            .send(ModelStreamChunk::Text {
                delta: "done".into(),
            })
            .await;
        let _ = tx
            .send(ModelStreamChunk::Done {
                usage: usage.clone(),
            })
            .await;

        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".into())],
            usage,
            stop_reason: StopReason::EndTurn,
        })
    }
}

#[tokio::test]
async fn e2e_event_coverage() {
    let model = Arc::new(E2EModelAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "test input".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    let event_types: Vec<&str> = events
        .iter()
        .map(|e| match e {
            RuntimeEvent::RunStarted { .. } => "RunStarted",
            RuntimeEvent::ModelCallStarted { .. } => "ModelCallStarted",
            RuntimeEvent::ModelStreamChunk { .. } => "ModelStreamChunk",
            RuntimeEvent::ModelCallCompleted { .. } => "ModelCallCompleted",
            RuntimeEvent::ToolCallStarted { .. } => "ToolCallStarted",
            RuntimeEvent::ToolCallUpdate { .. } => "ToolCallUpdate",
            RuntimeEvent::ToolCallCompleted { .. } => "ToolCallCompleted",
            RuntimeEvent::ToolCallFailed { .. } => "ToolCallFailed",
            RuntimeEvent::AsyncToolStarted { .. } => "AsyncToolStarted",
            RuntimeEvent::AsyncToolProgress { .. } => "AsyncToolProgress",
            RuntimeEvent::AsyncToolCompleted { .. } => "AsyncToolCompleted",
            RuntimeEvent::SkillContentRead { .. } => "SkillContentRead",
            RuntimeEvent::ApprovalRequested { .. } => "ApprovalRequested",
            RuntimeEvent::ApprovalGranted { .. } => "ApprovalGranted",
            RuntimeEvent::ApprovalDenied { .. } => "ApprovalDenied",
            RuntimeEvent::BudgetWarning { .. } => "BudgetWarning",
            RuntimeEvent::RuntimeWarning { .. } => "RuntimeWarning",
            RuntimeEvent::SkillDependencyError { .. } => "SkillDependencyError",
            RuntimeEvent::SkillMissingCapabilities { .. } => "SkillMissingCapabilities",
            RuntimeEvent::ContextCompacted { .. } => "ContextCompacted",
            RuntimeEvent::SubAgentStarted { .. } => "SubAgentStarted",
            RuntimeEvent::SubAgentCompleted { .. } => "SubAgentCompleted",
            RuntimeEvent::SubAgentFailed { .. } => "SubAgentFailed",
            RuntimeEvent::RunCompleted { .. } => "RunCompleted",
            RuntimeEvent::RunFailed { .. } => "RunFailed",
        })
        .collect();

    assert!(
        event_types.contains(&"RunStarted"),
        "missing RunStarted event"
    );
    assert!(
        event_types.contains(&"ModelCallStarted"),
        "missing ModelCallStarted"
    );
    assert!(
        event_types.contains(&"ModelCallCompleted"),
        "missing ModelCallCompleted"
    );
    assert!(
        event_types.contains(&"ModelStreamChunk"),
        "missing ModelStreamChunk"
    );
    assert!(
        event_types.contains(&"ToolCallStarted"),
        "missing ToolCallStarted"
    );
    assert!(
        event_types.contains(&"ToolCallCompleted"),
        "missing ToolCallCompleted"
    );
    assert!(
        event_types.contains(&"RunCompleted"),
        "missing RunCompleted"
    );

    let first = &event_types[0];
    let last = event_types.last().unwrap();
    assert_eq!(*first, "RunStarted");
    assert_eq!(*last, "RunCompleted");
}

#[tokio::test]
async fn e2e_thinking_boundaries_preserve_order() {
    let model = Arc::new(ThinkingBoundaryModel);
    let registry = ToolRegistry::new();

    let (handle, mut rx) = AgentRun::start(test_config(), "test thinking".into(), model, registry);

    let mut chunks = Vec::new();
    while let Some(event) = rx.recv().await {
        if let RuntimeEvent::ModelStreamChunk { delta } = event {
            chunks.push(delta);
        }
    }
    handle.wait().await;

    assert!(matches!(chunks[0], ModelStreamChunk::ThinkingStart));
    assert!(matches!(
        &chunks[1],
        ModelStreamChunk::Thinking { delta } if delta == "checking"
    ));
    assert!(matches!(chunks[2], ModelStreamChunk::ThinkingEnd));
    assert!(matches!(
        &chunks[3],
        ModelStreamChunk::Text { delta } if delta == "done"
    ));
    assert!(matches!(chunks[4], ModelStreamChunk::Done { .. }));
}

#[tokio::test]
async fn e2e_event_serialization_round_trip() {
    let model = Arc::new(E2EModelAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "test".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    for event in &events {
        let json_str = serde_json::to_string(event)
            .unwrap_or_else(|e| panic!("failed to serialize {:?}: {}", event, e));

        let deserialized: RuntimeEvent = serde_json::from_str(&json_str)
            .unwrap_or_else(|e| panic!("failed to deserialize '{}': {}", json_str, e));

        let re_serialized = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json_str, re_serialized, "round-trip mismatch");
    }
}

#[tokio::test]
async fn e2e_config_serialization() {
    let config = test_config();
    let json = serde_json::to_string(&config).expect("config should serialize");
    let deserialized: AgentConfig = serde_json::from_str(&json).expect("config should deserialize");
    assert_eq!(deserialized.system_prompt, config.system_prompt);
    assert_eq!(deserialized.model.model, config.model.model);
    assert_eq!(deserialized.max_steps, config.max_steps);
}

#[tokio::test]
async fn e2e_run_state_serialization() {
    let tool_call = agent_runtime_core::tool::ToolCall {
        id: "call_state".into(),
        name: "async_echo".into(),
        input: json!({"text": "hello"}),
    };
    let poll = Arc::new(|| {
        Box::pin(async {
            Ok(JobStatus::Pending {
                progress: None,
                message: None,
            })
        })
            as std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<JobStatus, ToolError>> + Send>,
            >
    });
    let state = agent_runtime_core::run::RunState {
        run_id: agent_runtime_core::run::RunId::new(),
        schema_version: "0.1".into(),
        config: test_config(),
        messages: vec![Message {
            role: agent_runtime_core::model::Role::User,
            content: vec![ContentBlock::Text("hello".into())],
        }],
        available_tools: Vec::new(),
        step: 1,
        status: agent_runtime_core::run::RunStatus::WaitingForAsyncTool {
            tool_call: tool_call.clone(),
            job_handle: JobHandle {
                job_id: "job_state".into(),
                poll: Some(poll),
                poll_interval: Duration::from_millis(25),
                timeout: Some(Duration::from_secs(5)),
                webhook: None,
            },
            since: std::time::Instant::now(),
        },
        budget_used: agent_runtime_core::budget::BudgetUsage {
            tokens_used: 1500,
            tool_calls_used: 3,
            cost_usd: 0.042,
        },
    };

    let json = serde_json::to_string(&state).expect("run state should serialize");
    let deserialized: agent_runtime_core::run::RunState =
        serde_json::from_str(&json).expect("run state should deserialize");

    assert_eq!(deserialized.schema_version, "0.1");
    assert_eq!(deserialized.step, 1);
    assert_eq!(deserialized.messages.len(), 1);
    assert_eq!(deserialized.budget_used.tokens_used, 1500);
    assert!(deserialized.available_tools.is_empty());
    match deserialized.status {
        agent_runtime_core::run::RunStatus::WaitingForAsyncTool {
            tool_call,
            job_handle,
            ..
        } => {
            assert_eq!(tool_call.id, "call_state");
            assert_eq!(job_handle.job_id, "job_state");
            assert_eq!(job_handle.poll_interval, Duration::from_millis(25));
        }
        other => panic!("expected WaitingForAsyncTool, got {:?}", other),
    }
}

#[tokio::test]
async fn e2e_budget_warning_event() {
    let mut config = test_config();
    config.budget.max_tokens = Some(1);

    let model = Arc::new(E2EModelAdapter);
    let registry = ToolRegistry::new();
    let (handle, mut rx) = AgentRun::start(config, "test".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(
        events.iter().any(|event| matches!(
            event,
            RuntimeEvent::BudgetWarning {
                used,
                limit
            } if used.tokens_used > limit.max_tokens.unwrap_or_default()
        )),
        "missing BudgetWarning"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            RuntimeEvent::RunFailed { error } if error == "budget_exceeded"
        )),
        "missing budget_exceeded RunFailed"
    );
}

#[tokio::test]
async fn e2e_skill_content_read_event() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let skill_dir = tmp.path().join("demo_skill");
    std::fs::create_dir_all(&skill_dir).expect("skill dir should be created");
    let skill_md = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_md,
        "# Demo skill\nUse this skill for e2e validation.",
    )
    .expect("skill file should be written");

    let tool = ReadFileTool::new();
    tool.register_skill("demo_skill".into(), skill_md.clone())
        .await;

    let (event_tx, mut event_rx) = mpsc::channel(16);
    let ctx = ToolContext {
        run_id: agent_runtime_core::run::RunId::new(),
        run_depth: 0,
        tool_call_id: "read_skill".into(),
        on_update: None,
        event_tx: Some(event_tx),
        webhook_base_url: None,
    };

    tool.execute(json!({"path": skill_md.to_str().unwrap()}), &ctx)
        .await
        .expect("read_file should read skill content");

    let event = event_rx
        .recv()
        .await
        .expect("skill content event should be emitted");
    assert!(matches!(
        event,
        RuntimeEvent::SkillContentRead {
            skill_name,
            ..
        } if skill_name == "demo_skill"
    ));
}

#[tokio::test]
async fn e2e_read_file_known_risk_boundary_is_visible() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let non_skill_file = tmp.path().join("outside-skill.txt");
    std::fs::write(&non_skill_file, "host-readable content")
        .expect("non-skill file should be written");

    let tool = ReadFileTool::new();
    let (event_tx, mut event_rx) = mpsc::channel(16);
    let ctx = ToolContext {
        run_id: agent_runtime_core::run::RunId::new(),
        run_depth: 0,
        tool_call_id: "read_non_skill".into(),
        on_update: None,
        event_tx: Some(event_tx),
        webhook_base_url: None,
    };

    let output = tool
        .execute(json!({"path": non_skill_file.to_str().unwrap()}), &ctx)
        .await
        .expect("v0.1 read_file intentionally allows host-readable non-skill files");

    assert!(matches!(
        output,
        ToolOutput::Immediate(Value::String(content)) if content == "host-readable content"
    ));
    assert!(
        event_rx.try_recv().is_err(),
        "non-skill reads are allowed in v0.1 but do not emit SkillContentRead"
    );
}

#[tokio::test]
async fn e2e_budget_usage_serialization() {
    let usage = agent_runtime_core::budget::BudgetUsage {
        tokens_used: 1500,
        tool_calls_used: 3,
        cost_usd: 0.042,
    };
    let json = serde_json::to_string(&usage).expect("usage should serialize");
    let deserialized: agent_runtime_core::budget::BudgetUsage =
        serde_json::from_str(&json).expect("usage should deserialize");
    assert_eq!(deserialized.tokens_used, 1500);
    assert_eq!(deserialized.tool_calls_used, 3);
    assert!((deserialized.cost_usd - 0.042).abs() < 0.001);
}

struct AsyncToolAdapter;

#[async_trait::async_trait]
impl ModelAdapter for AsyncToolAdapter {
    async fn stream(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
        };

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if has_tool_result {
            let _ = tx
                .send(ModelStreamChunk::Text {
                    delta: "Done.".into(),
                })
                .await;
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Done.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
            })
        } else {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "async_echo".into(),
                    input: json!({"text": "hello"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
            })
        }
    }
}

struct AsyncEchoTool;

#[async_trait::async_trait]
impl Tool for AsyncEchoTool {
    fn name(&self) -> &str {
        "async_echo"
    }
    fn description(&self) -> &str {
        "async echo"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: false,
            requires_approval: false,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let has_reported_progress = Arc::new(AtomicBool::new(false));
        let poll_fn = move || {
            let input = input.clone();
            let has_reported_progress = Arc::clone(&has_reported_progress);
            Box::pin(async move {
                if !has_reported_progress.swap(true, Ordering::SeqCst) {
                    Ok(JobStatus::Pending {
                        progress: Some(0.5),
                        message: Some("halfway".into()),
                    })
                } else {
                    Ok(JobStatus::Completed(input))
                }
            })
                as std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<JobStatus, ToolError>> + Send>,
                >
        };

        Ok(ToolOutput::AsyncJob(JobHandle {
            job_id: "job_e2e_1".into(),
            poll: Some(Arc::new(poll_fn)),
            poll_interval: Duration::from_millis(10),
            timeout: Some(Duration::from_secs(5)),
            webhook: None,
        }))
    }
}

#[tokio::test]
async fn e2e_async_tool_events() {
    let model = Arc::new(AsyncToolAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(AsyncEchoTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "test".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    let event_types: Vec<&str> = events
        .iter()
        .map(|e| match e {
            RuntimeEvent::AsyncToolStarted { .. } => "AsyncToolStarted",
            RuntimeEvent::AsyncToolProgress { .. } => "AsyncToolProgress",
            RuntimeEvent::AsyncToolCompleted { .. } => "AsyncToolCompleted",
            RuntimeEvent::RunCompleted { .. } => "RunCompleted",
            _ => "other",
        })
        .collect();

    assert!(
        event_types.contains(&"AsyncToolStarted"),
        "missing AsyncToolStarted"
    );
    assert!(
        event_types.contains(&"AsyncToolProgress"),
        "missing AsyncToolProgress"
    );
    assert!(
        event_types.contains(&"AsyncToolCompleted"),
        "missing AsyncToolCompleted"
    );
    assert!(
        event_types.contains(&"RunCompleted"),
        "missing RunCompleted"
    );
}

struct ApprovalAdapter;

#[async_trait::async_trait]
impl ModelAdapter for ApprovalAdapter {
    async fn stream(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
        };

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if has_tool_result {
            let denied = messages.iter().any(|m| {
                m.content.iter().any(|c| match c {
                    ContentBlock::ToolResult { content, .. } => content
                        .get("error")
                        .and_then(|value| value.as_str())
                        .is_some_and(|error| error == "tool call denied by user"),
                    _ => false,
                })
            });
            let output = if denied { "Denied." } else { "Approved." };
            let _ = tx
                .send(ModelStreamChunk::Text {
                    delta: output.into(),
                })
                .await;
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(output.into())],
                usage,
                stop_reason: StopReason::EndTurn,
            })
        } else {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "guarded".into(),
                    input: json!({"action": "delete"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
            })
        }
    }
}

struct GuardedTool;

#[async_trait::async_trait]
impl Tool for GuardedTool {
    fn name(&self) -> &str {
        "guarded"
    }
    fn description(&self) -> &str {
        "requires approval"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: true,
            requires_approval: true,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(
            json!({"approved": true, "input": input}),
        ))
    }
}

#[tokio::test]
async fn e2e_approval_flow() {
    let model = Arc::new(ApprovalAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(GuardedTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "delete it".into(), model, registry);

    let mut saw_approval_requested = false;
    let mut events = Vec::new();

    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ApprovalRequested { .. }) {
            saw_approval_requested = true;
            handle.respond_approval(handle.run_id, true).await;
        }
        events.push(event);
    }
    handle.wait().await;

    assert!(saw_approval_requested, "expected ApprovalRequested event");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ApprovalGranted { .. })),
        "missing ApprovalGranted"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "missing RunCompleted"
    );
}

#[tokio::test]
async fn e2e_approval_denied_flow_completes_without_executing_tool() {
    let model = Arc::new(ApprovalAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(GuardedTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "delete it".into(), model, registry);

    let mut saw_approval_requested = false;
    let mut events = Vec::new();

    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ApprovalRequested { .. }) {
            saw_approval_requested = true;
            handle.respond_approval(handle.run_id, false).await;
        }
        events.push(event);
    }
    handle.wait().await;

    assert!(saw_approval_requested, "expected ApprovalRequested event");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ApprovalDenied { .. })),
        "missing ApprovalDenied"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { tool, .. } if tool == "guarded")),
        "denied tool should not start execution"
    );
    assert!(
        !events.iter().any(
            |e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "guarded")
        ),
        "denied tool should not complete execution"
    );
    assert!(
        events.iter().any(|e| {
            matches!(
                e,
                RuntimeEvent::RunCompleted { output } if output == &json!("Denied.")
            )
        }),
        "denial should feed a tool result back to the model and allow RunCompleted"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunFailed { .. })),
        "approval denial by itself should not fail the run"
    );
}
