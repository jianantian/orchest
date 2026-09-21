use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use orchest::budget::BudgetConfig;
use orchest::events::{RunFailureKind, RuntimeEvent};
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse, ModelSpec,
    OptionAdjustment, RequestOptions, StopReason, StreamEvent, ThinkingLevel, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun, ModelConfig, RuntimeConfig, SkillsConfig};
use orchest::tool::async_job::{JobHandle, JobStatus};
use orchest::tool::builtin::ReadFileTool;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput,
    ToolSource,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

fn test_config() -> AgentConfig {
    AgentConfig {
        name: "test-agent".into(),
        system_prompt: "You are a test assistant.".into(),
        model: ModelConfig {
            spec: ModelSpec {
                provider: "test".into(),
                model: "test-model".into(),
                api_key_env: None,
                api_url: None,
                max_tokens: Some(100),
                context_window_size: None,
            },
            options: RequestOptions::default(),
        },
        budget: BudgetConfig {
            max_tokens: Some(100_000),
            max_tool_calls: Some(10),
            max_duration: Some(Duration::from_secs(30)),
            max_cost_usd: Some(1.0),
        },
        skills: SkillsConfig::default(),
        runtime: RuntimeConfig {
            max_steps: 10,
            ..RuntimeConfig::default()
        },
        hooks: vec![],
        retry_policy: None,
        handoffs: vec![],
        session_store: None,
        session_id: None,
        supervision_strategy: Default::default(),
    }
}

struct E2EModelAdapter;

#[async_trait::async_trait]
impl ModelAdapter for E2EModelAdapter {
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
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
            ..Default::default()
        };

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if has_tool_result {
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Text {
                        delta: "Task ".into(),
                    })
                    .await;
            }
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Text {
                        delta: "complete.".into(),
                    })
                    .await;
            }
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Task complete.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "echo".into(),
                    input: json!({"text": "hello"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

struct CapturingOptionsModel {
    seen_options: Arc<Mutex<Option<RequestOptions>>>,
}

#[async_trait::async_trait]
impl ModelAdapter for CapturingOptionsModel {
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
        _messages: &[Message],
        _tools: &[ToolDef],
        options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        *self.seen_options.lock().await = Some(options.clone());
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![OptionAdjustment {
                option: "thinking".into(),
                requested: json!("high"),
                applied: json!("off"),
                reason: "test_adjustment".into(),
            }],
        })
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
            approval: Approval::Never,
            execution_mode: orchest::tool::ToolExecutionMode::Normal,
            parallelism: orchest::tool::ToolParallelism::Serial,
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
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 4,
            output_tokens: 8,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx.send(StreamEvent::ThinkingStart).await;
        }
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Thinking {
                    delta: "checking".into(),
                })
                .await;
        }
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::ThinkingEnd {
                    signature: None,
                    provider_details: None,
                })
                .await;
        }
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Text {
                    delta: "done".into(),
                })
                .await;
        }
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".into())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
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
            RuntimeEvent::ToolCallRetry { .. } => "ToolCallRetry",
            RuntimeEvent::ToolCallBatchStarted { .. } => "ToolCallBatchStarted",
            RuntimeEvent::ToolCallBatchItemStarted { .. } => "ToolCallBatchItemStarted",
            RuntimeEvent::ToolCallBatchItemCompleted { .. } => "ToolCallBatchItemCompleted",
            RuntimeEvent::AsyncToolStarted { .. } => "AsyncToolStarted",
            RuntimeEvent::AsyncToolProgress { .. } => "AsyncToolProgress",
            RuntimeEvent::AsyncToolCompleted { .. } => "AsyncToolCompleted",
            RuntimeEvent::SkillContentRead { .. } => "SkillContentRead",
            RuntimeEvent::ApprovalRequested { .. } => "ApprovalRequested",
            RuntimeEvent::ApprovalGranted { .. } => "ApprovalGranted",
            RuntimeEvent::ApprovalDenied { .. } => "ApprovalDenied",
            RuntimeEvent::BudgetWarning { .. } => "BudgetWarning",
            RuntimeEvent::RuntimeWarning { .. } => "RuntimeWarning",
            RuntimeEvent::SkillMissingCapabilities { .. } => "SkillMissingCapabilities",
            RuntimeEvent::SkillLoadWarning { .. } => "SkillLoadWarning",
            RuntimeEvent::ContextCompacted { .. } => "ContextCompacted",
            RuntimeEvent::SubAgentStarted { .. } => "SubAgentStarted",
            RuntimeEvent::SubAgentCompleted { .. } => "SubAgentCompleted",
            RuntimeEvent::SubAgentFailed { .. } => "SubAgentFailed",
            RuntimeEvent::RunCompleted { .. } => "RunCompleted",
            RuntimeEvent::RunFailed { .. } => "RunFailed",
            RuntimeEvent::RunAborted { .. } => "RunAborted",
            RuntimeEvent::ChildRunEvent { .. } => "ChildRunEvent",
            RuntimeEvent::SubAgentEvent { .. } => "SubAgentEvent",
            RuntimeEvent::HookPanicked { .. } => "HookPanicked",
            RuntimeEvent::ModelRetry { .. } => "ModelRetry",
            RuntimeEvent::AgentUpdated { .. } => "AgentUpdated",
            RuntimeEvent::EventsDropped { .. } => "EventsDropped",
            RuntimeEvent::RunRestarted { .. } => "RunRestarted",
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
async fn agent_config_request_options_reach_model_complete() {
    let seen_options = Arc::new(Mutex::new(None));
    let model = Arc::new(CapturingOptionsModel {
        seen_options: Arc::clone(&seen_options),
    });
    let registry = ToolRegistry::new();
    let mut config = test_config();
    config.model.options.thinking = ThinkingLevel::High;
    config.model.options.max_tokens = Some(777);

    let (handle, mut rx) = AgentRun::start(config, "test request options".into(), model, registry);
    while rx.recv().await.is_some() {}
    handle.wait().await;

    let options = seen_options
        .lock()
        .await
        .clone()
        .expect("model should receive request options");
    assert_eq!(options.thinking, ThinkingLevel::High);
    assert_eq!(options.max_tokens, Some(777));
}

#[tokio::test]
async fn runtime_event_model_call_completed_includes_option_adjustments() {
    let model = Arc::new(CapturingOptionsModel {
        seen_options: Arc::new(Mutex::new(None)),
    });
    let registry = ToolRegistry::new();

    let (handle, mut rx) = AgentRun::start(
        test_config(),
        "test option adjustments".into(),
        model,
        registry,
    );
    let mut adjustments = None;
    while let Some(event) = rx.recv().await {
        if let RuntimeEvent::ModelCallCompleted {
            option_adjustments, ..
        } = event
        {
            adjustments = Some(option_adjustments);
        }
    }
    handle.wait().await;

    let adjustments = adjustments.expect("model completion event should be emitted");
    assert_eq!(adjustments.len(), 1);
    assert_eq!(adjustments[0].reason, "test_adjustment");
}

#[test]
fn serialization_token_usage_contains_extended_fields() {
    let value = serde_json::to_value(TokenUsage::default()).expect("serialize TokenUsage");
    assert!(value.get("input_tokens").is_some());
    assert!(value.get("output_tokens").is_some());
    assert!(value.get("reasoning_tokens").is_some());
    assert!(value.get("cache_read_tokens").is_some());
    assert!(value.get("cache_write_tokens").is_some());
    assert!(value.get("details").is_some());
}

#[test]
fn serialization_stream_event_thinking_end_preserves_signature_and_details() {
    let event = StreamEvent::ThinkingEnd {
        signature: Some("sig".into()),
        provider_details: Some(json!({"type": "reasoning"})),
    };
    let restored: StreamEvent =
        serde_json::from_value(serde_json::to_value(&event).expect("serialize"))
            .expect("deserialize");
    assert_eq!(restored, event);
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

    assert!(matches!(chunks[0], StreamEvent::ThinkingStart));
    assert!(matches!(
        &chunks[1],
        StreamEvent::Thinking { delta } if delta == "checking"
    ));
    assert!(matches!(chunks[2], StreamEvent::ThinkingEnd { .. }));
    assert!(matches!(
        &chunks[3],
        StreamEvent::Text { delta } if delta == "done"
    ));
    assert!(matches!(chunks[4], StreamEvent::Done { .. }));
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
            .unwrap_or_else(|e| panic!("failed to serialize {event:?}: {e}"));

        let deserialized: RuntimeEvent = serde_json::from_str(&json_str)
            .unwrap_or_else(|e| panic!("failed to deserialize '{json_str}': {e}"));

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
    assert_eq!(deserialized.model.spec.model, config.model.spec.model);
    assert_eq!(deserialized.runtime.max_steps, config.runtime.max_steps);
}

#[tokio::test]
async fn e2e_run_state_serialization() {
    let tool_call = orchest::tool::ToolCall {
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
    let state = orchest::run::RunState {
        run_id: orchest::run::RunId::new(),
        schema_version: "0.1".into(),
        config: test_config(),
        messages: vec![Message {
            role: orchest::model::Role::User,
            content: vec![ContentBlock::Text("hello".into())],
        }],
        available_tools: Vec::new(),
        step: 1,
        status: orchest::run::RunStatus::WaitingForAsyncTool {
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
        budget_used: orchest::budget::BudgetUsage {
            tokens_used: 1500,
            tool_calls_used: 3,
            cost_usd: 0.042,
        },
    };

    let json = serde_json::to_string(&state).expect("run state should serialize");
    let deserialized: orchest::run::RunState =
        serde_json::from_str(&json).expect("run state should deserialize");

    assert_eq!(deserialized.schema_version, "0.1");
    assert_eq!(deserialized.step, 1);
    assert_eq!(deserialized.messages.len(), 1);
    assert_eq!(deserialized.budget_used.tokens_used, 1500);
    assert!(deserialized.available_tools.is_empty());
    match deserialized.status {
        orchest::run::RunStatus::WaitingForAsyncTool {
            tool_call,
            job_handle,
            ..
        } => {
            assert_eq!(tool_call.id, "call_state");
            assert_eq!(job_handle.job_id, "job_state");
            assert_eq!(job_handle.poll_interval, Duration::from_millis(25));
        }
        other => panic!("expected WaitingForAsyncTool, got {other:?}"),
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
            RuntimeEvent::RunFailed { error, kind }
                if error.starts_with("budget_exceeded") && *kind == RunFailureKind::BudgetExceeded
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
        tool_call_id: "read_skill".into(),
        event_tx: Some(event_tx),
        ..ToolContext::oneshot()
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
        tool_call_id: "read_non_skill".into(),
        event_tx: Some(event_tx),
        ..ToolContext::oneshot()
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
    let usage = orchest::budget::BudgetUsage {
        tokens_used: 1500,
        tool_calls_used: 3,
        cost_usd: 0.042,
    };
    let json = serde_json::to_string(&usage).expect("usage should serialize");
    let deserialized: orchest::budget::BudgetUsage =
        serde_json::from_str(&json).expect("usage should deserialize");
    assert_eq!(deserialized.tokens_used, 1500);
    assert_eq!(deserialized.tool_calls_used, 3);
    assert!((deserialized.cost_usd - 0.042).abs() < 0.001);
}

struct AsyncToolAdapter;

#[async_trait::async_trait]
impl ModelAdapter for AsyncToolAdapter {
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
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
            ..Default::default()
        };

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if has_tool_result {
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Text {
                        delta: "Done.".into(),
                    })
                    .await;
            }
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("Done.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "async_echo".into(),
                    input: json!({"text": "hello"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
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
            approval: Approval::Never,
            execution_mode: orchest::tool::ToolExecutionMode::Normal,
            parallelism: orchest::tool::ToolParallelism::Serial,
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
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
            ..Default::default()
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
                        .and_then(|value| value.get("message"))
                        .and_then(|value| value.as_str())
                        .is_some_and(|message| message == "tool call denied by user"),
                    _ => false,
                })
            });
            let output = if denied { "Denied." } else { "Approved." };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Text {
                        delta: output.into(),
                    })
                    .await;
            }
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(output.into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "guarded".into(),
                    input: json!({"action": "delete"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
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
            approval: Approval::Always,
            execution_mode: orchest::tool::ToolExecutionMode::Normal,
            parallelism: orchest::tool::ToolParallelism::Serial,
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
            let _ = handle.respond_approval(handle.run_id, true).await;
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
            let _ = handle.respond_approval(handle.run_id, false).await;
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
                RuntimeEvent::RunCompleted { output, .. } if output == &json!("Denied.")
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
