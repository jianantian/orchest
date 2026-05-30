use super::*;
use crate::budget::BudgetConfig;
use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse, ModelSpec,
    ModelStreamChunk, RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
};
use crate::tool::async_job::{JobHandle, JobStatus};
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    JsonSchema, Tool, ToolCall, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput,
    ToolSource,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

struct FakeModelAdapter {
    call_count: AtomicU32,
}

impl FakeModelAdapter {
    fn final_answer() -> Self {
        Self {
            call_count: AtomicU32::new(0),
        }
    }
}

#[async_trait::async_trait]
impl ModelAdapter for FakeModelAdapter {
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
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Text {
                    delta: "hello".into(),
                })
                .await;
        }
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        if count == 0 {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("hello".into())],
                usage,
                stop_reason: StopReason::EndTurn,
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

fn test_config() -> AgentConfig {
    AgentConfig {
        system_prompt: "you are helpful".into(),
        model: config::ModelConfig {
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
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        },
        skills: config::SkillsConfig::default(),
        runtime: config::RuntimeConfig {
            max_steps: 10,
            ..config::RuntimeConfig::default()
        },
        hooks: vec![],
        retry_policy: None,
    }
}

#[tokio::test]
async fn run_loop_final_answer() {
    let model = Arc::new(FakeModelAdapter::final_answer());
    let registry = ToolRegistry::new();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(matches!(events[0], RuntimeEvent::RunStarted { .. }));
    assert!(matches!(
        events[1],
        RuntimeEvent::ModelCallStarted { step: 0 }
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
}

#[tokio::test]
async fn run_loop_max_steps() {
    let model = Arc::new(FakeModelAdapter::final_answer());
    let registry = ToolRegistry::new();

    let mut config = test_config();
    config.runtime.max_steps = 0;

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(events.iter().any(|e| matches!(
        e,
        RuntimeEvent::RunFailed { error } if error == "max_steps_reached"
    )));
}

struct ToolCallModelAdapter;

#[async_trait::async_trait]
impl ModelAdapter for ToolCallModelAdapter {
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
        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
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

struct FakeTool {
    name: &'static str,
    requires_approval: bool,
}

impl FakeTool {
    fn echo() -> Self {
        Self {
            name: "echo",
            requires_approval: false,
        }
    }

    fn guarded(name: &'static str) -> Self {
        Self {
            name,
            requires_approval: true,
        }
    }
}

#[async_trait::async_trait]
impl Tool for FakeTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "fake tool"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        if self.requires_approval {
            &ToolMetadata {
                side_effect: true,
                requires_approval: true,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            }
        } else {
            &ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::InProcess,
            }
        }
    }
    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(input))
    }
}

#[tokio::test]
async fn run_loop_with_tool_call() {
    let model = Arc::new(ToolCallModelAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { tool, .. } if tool == "echo")));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "echo")));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
}

struct StructuredToolModelAdapter;

#[async_trait::async_trait]
impl ModelAdapter for StructuredToolModelAdapter {
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
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let tool_result = messages.iter().find_map(|message| {
            message.content.iter().find_map(|block| match block {
                ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
        });

        if let Some(content) = tool_result {
            assert_eq!(content, json!({"summary": "compact for model"}));
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "research".into(),
                    input: json!({"question": "q"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

struct StructuredTool;

#[async_trait::async_trait]
impl Tool for StructuredTool {
    fn name(&self) -> &str {
        "research"
    }
    fn description(&self) -> &str {
        "structured output tool"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
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
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Structured {
            model_output: json!({"summary": "compact for model"}),
            details: json!({
                "summary": "compact for model",
                "raw_results": ["large detail only for events"]
            }),
        })
    }
}

#[tokio::test]
async fn structured_tool_output_sends_details_to_events_and_model_output_to_model() {
    let model = Arc::new(StructuredToolModelAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(StructuredTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    let detail_output = events.iter().find_map(|event| match event {
        RuntimeEvent::ToolCallCompleted { tool, output, .. } if tool == "research" => Some(output),
        _ => None,
    });
    assert_eq!(
        detail_output,
        Some(&json!({
            "summary": "compact for model",
            "raw_results": ["large detail only for events"]
        }))
    );
}

struct ApprovalModelAdapter;

#[async_trait::async_trait]
impl ModelAdapter for ApprovalModelAdapter {
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
        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "write_file".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn approval_gate_approved() {
    let model = Arc::new(ApprovalModelAdapter);
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(FakeTool::guarded("write_file")))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    loop {
        match rx.recv().await {
            Some(RuntimeEvent::ApprovalRequested { .. }) => {
                events.push(RuntimeEvent::ApprovalRequested {
                    tool_call: ToolCall {
                        id: String::new(),
                        name: String::new(),
                        input: json!(null),
                    },
                });
                handle.respond_approval(handle.run_id, true).await.unwrap();
            }
            Some(event) => events.push(event),
            None => break,
        }
    }
    handle.wait().await;

    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ApprovalRequested { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ApprovalGranted { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { .. })));
}

#[tokio::test]
async fn approval_gate_denied() {
    let model = Arc::new(ApprovalModelAdapter);
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(FakeTool::guarded("write_file")))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    loop {
        match rx.recv().await {
            Some(RuntimeEvent::ApprovalRequested { .. }) => {
                events.push(RuntimeEvent::ApprovalRequested {
                    tool_call: ToolCall {
                        id: String::new(),
                        name: String::new(),
                        input: json!(null),
                    },
                });
                handle.respond_approval(handle.run_id, false).await.unwrap();
            }
            Some(event) => events.push(event),
            None => break,
        }
    }
    handle.wait().await;

    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ApprovalDenied { .. })));
    assert!(!events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { .. })));
}

struct AsyncTool {
    polls_until_done: AtomicU32,
}

#[async_trait::async_trait]
impl Tool for AsyncTool {
    fn name(&self) -> &str {
        "async_op"
    }
    fn description(&self) -> &str {
        "async operation"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
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
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let remaining = Arc::new(AtomicU32::new(self.polls_until_done.load(Ordering::SeqCst)));
        let poll_fn = {
            let remaining = Arc::clone(&remaining);
            move || {
                let remaining = Arc::clone(&remaining);
                Box::pin(async move {
                    let left = remaining.fetch_sub(1, Ordering::SeqCst);
                    if left <= 1 {
                        Ok(JobStatus::Completed(json!("async_result")))
                    } else {
                        Ok(JobStatus::Pending {
                            progress: Some(1.0 - (left as f32 / 3.0)),
                            message: Some("working".into()),
                        })
                    }
                })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<JobStatus, ToolError>> + Send>,
                    >
            }
        };
        Ok(ToolOutput::AsyncJob(JobHandle {
            job_id: "job-1".into(),
            poll: Some(Arc::new(poll_fn)),
            poll_interval: std::time::Duration::from_millis(1),
            timeout: None,
            webhook: None,
        }))
    }
}

struct AsyncToolModelAdapter;

#[async_trait::async_trait]
impl ModelAdapter for AsyncToolModelAdapter {
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
        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "async_op".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn async_job_polling() {
    let model = Arc::new(AsyncToolModelAdapter);
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(AsyncTool {
            polls_until_done: AtomicU32::new(3),
        }))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::AsyncToolStarted { tool, job_id } if tool == "async_op" && job_id == "job-1")));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::AsyncToolProgress { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::AsyncToolCompleted { tool, .. } if tool == "async_op")));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
}

struct ToolSearchModel {
    call_count: AtomicU32,
}

#[async_trait::async_trait]
impl ModelAdapter for ToolSearchModel {
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
        tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if count == 0 {
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0].name, "search_tools");
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "search_1".into(),
                    name: "search_tools".into(),
                    input: json!({"query": "async operation"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            assert!(tools.iter().any(|tool| tool.name == "async_op"));
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn tool_search_enabled_loads_schemas_progressively() {
    let mut config = test_config();
    config.runtime.tool_search_enabled = true;
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(AsyncTool {
            polls_until_done: AtomicU32::new(1),
        }))
        .unwrap();
    let model = Arc::new(ToolSearchModel {
        call_count: AtomicU32::new(0),
    });
    let (handle, mut rx) = AgentRun::start(config, "find tool".into(), model, registry);
    while rx.recv().await.is_some() {}
    handle.wait().await;
}

struct CompactingModel {
    call_count: AtomicU32,
}

#[async_trait::async_trait]
impl ModelAdapter for CompactingModel {
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
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = if count == 0 {
            TokenUsage {
                input_tokens: 90,
                output_tokens: 20,
                ..Default::default()
            }
        } else {
            TokenUsage {
                input_tokens: 1,
                output_tokens: 3,
                ..Default::default()
            }
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if messages
                .iter()
                .any(|message| matches!(message.role, Role::User)
                    && message.content.iter().any(|block| matches!(block, ContentBlock::Text(text) if text.contains("历史对话记录"))))
            {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("摘要".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
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

#[tokio::test]
async fn context_compaction_emits_event() {
    let mut config = test_config();
    config.runtime.compaction = Some(config::CompactionConfig {
        threshold: 0.5,
        recent_messages: 0,
    });
    config.model.spec.context_window_size = Some(100);
    let model = Arc::new(CompactingModel {
        call_count: AtomicU32::new(0),
    });
    let registry = ToolRegistry::new();
    let (handle, mut rx) = AgentRun::start(config, "compact".into(), model, registry);
    let mut saw_compacted = false;
    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ContextCompacted { .. }) {
            saw_compacted = true;
        }
    }
    handle.wait().await;
    assert!(saw_compacted);
}

struct WebhookTool;

#[async_trait::async_trait]
impl Tool for WebhookTool {
    fn name(&self) -> &str {
        "webhook_tool"
    }
    fn description(&self) -> &str {
        "webhook async tool"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
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
    async fn execute(
        &self,
        _input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let job_id = uuid::Uuid::new_v4().to_string();
        let url = format!(
            "{}/webhooks/async-job/{}",
            ctx.webhook_base_url.as_ref().ok_or_else(|| ToolError {
                message: "missing webhook base url".into(),
                code: None,
            })?,
            job_id
        );
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = reqwest::Client::new()
                .post(url)
                .json(&json!({"status": "completed", "result": {"ok": true}}))
                .send()
                .await;
        });
        Ok(ToolOutput::AsyncJob(JobHandle {
            job_id: job_id.clone(),
            poll: None,
            poll_interval: std::time::Duration::from_secs(1),
            timeout: Some(std::time::Duration::from_secs(5)),
            webhook: Some(crate::tool::async_job::WebhookConfig {
                expected_job_id: job_id,
            }),
        }))
    }
}

struct WebhookModel {
    call_count: AtomicU32,
}

#[async_trait::async_trait]
impl ModelAdapter for WebhookModel {
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
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if self.call_count.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "webhook_1".into(),
                    name: "webhook_tool".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            assert!(messages.iter().any(|message| {
                    message.content.iter().any(|block| {
                        matches!(block, ContentBlock::ToolResult { content, .. } if content["ok"] == true)
                    })
                }));
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn webhook_async_job_completes_without_polling() {
    let mut config = test_config();
    config.runtime.webhook_enabled = true;
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WebhookTool)).unwrap();
    let model = Arc::new(WebhookModel {
        call_count: AtomicU32::new(0),
    });
    let (handle, mut rx) = AgentRun::start(config, "run webhook".into(), model, registry);
    let mut completed = false;
    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::AsyncToolCompleted { .. }) {
            completed = true;
        }
    }
    handle.wait().await;
    assert!(completed);
}

struct AllowedToolsModel {
    target_tool: String,
}

#[async_trait::async_trait]
impl ModelAdapter for AllowedToolsModel {
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
        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: self.target_tool.clone(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn allowed_tools_filters_visibility_and_permits_execution() {
    let mut config = test_config();
    config.runtime.allowed_tools = Some(vec!["echo".into()]);

    let model = Arc::new(AllowedToolsModel {
        target_tool: "echo".into(),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();
    registry
        .register(Arc::new(FakeTool {
            name: "secret",
            requires_approval: false,
        }))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "echo")));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
}

#[tokio::test]
async fn allowed_tools_denies_disallowed_tool_by_name() {
    let mut config = test_config();
    config.runtime.allowed_tools = Some(vec!["echo".into()]);

    let model = Arc::new(AllowedToolsModel {
        target_tool: "secret".into(),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();
    registry
        .register(Arc::new(FakeTool {
            name: "secret",
            requires_approval: false,
        }))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool not allowed")
            ),
            "should emit ToolCallFailed with 'tool not allowed'"
        );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { tool, .. } if tool == "secret")),
        "ToolCallStarted must not be emitted for denied-by-policy tools"
    );
}

#[tokio::test]
async fn allowed_tools_empty_list_denies_all() {
    let mut config = test_config();
    config.runtime.allowed_tools = Some(vec![]);

    let model = Arc::new(AllowedToolsModel {
        target_tool: "echo".into(),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(events.iter().any(
        |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool not allowed")
    ),);
}

#[tokio::test]
async fn allowed_tools_none_permits_all() {
    let mut config = test_config();
    config.runtime.allowed_tools = None;

    let model = Arc::new(AllowedToolsModel {
        target_tool: "echo".into(),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "echo")));
}

struct SlowTool {
    metadata: ToolMetadata,
}

impl SlowTool {
    fn new() -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: Some(Duration::from_millis(50)),
                max_output_tokens: None,
                source: ToolSource::InProcess,
            },
        }
    }
}

#[async_trait::async_trait]
impl Tool for SlowTool {
    fn name(&self) -> &str {
        "slow"
    }
    fn description(&self) -> &str {
        "sleeps forever"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(ToolOutput::Immediate(json!("should not reach")))
    }
}

struct SlowToolModel;

#[async_trait::async_trait]
impl ModelAdapter for SlowToolModel {
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
        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "slow".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn tool_metadata_timeout_enforced() {
    let model = Arc::new(SlowToolModel);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(SlowTool::new())).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool execution timed out")
            ),
            "should emit ToolCallFailed with timeout error"
        );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run should continue after timeout"
    );
}

struct BigOutputTool {
    metadata: ToolMetadata,
}

impl BigOutputTool {
    fn new() -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: Some(10),
                source: ToolSource::InProcess,
            },
        }
    }
}

#[async_trait::async_trait]
impl Tool for BigOutputTool {
    fn name(&self) -> &str {
        "big_output"
    }
    fn description(&self) -> &str {
        "returns large output"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(Value::String("x".repeat(1000))))
    }
}

#[tokio::test]
async fn max_output_tokens_truncates_output() {
    let model = Arc::new(AllowedToolsModel {
        target_tool: "big_output".into(),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(BigOutputTool::new())).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    let completed = events.iter().find_map(|e| match e {
        RuntimeEvent::ToolCallCompleted { output, .. } => Some(output),
        _ => None,
    });
    assert!(completed.is_some(), "should have ToolCallCompleted");
    let output_str = completed.unwrap().as_str().unwrap();
    assert!(output_str.contains("[output truncated]"));
    assert!(output_str.len() < 1000);
}

struct MultiToolCallModel {
    call_count: AtomicU32,
}

#[async_trait::async_trait]
impl ModelAdapter for MultiToolCallModel {
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
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 5,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if count < 5 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: format!("call_{count}"),
                    name: "echo".into(),
                    input: json!({"n": count}),
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

#[tokio::test]
async fn max_tool_calls_boundary_enforced() {
    let mut config = test_config();
    config.budget.max_tool_calls = Some(2);

    let model = Arc::new(MultiToolCallModel {
        call_count: AtomicU32::new(0),
    });
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    let completed_count = events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::ToolCallCompleted { .. }))
        .count();
    assert_eq!(completed_count, 2, "should execute exactly max_tool_calls");

    let budget_exceeded = events.iter().any(
            |e| matches!(e, RuntimeEvent::ToolCallFailed { error, .. } if error == "tool call budget exceeded"),
        );
    assert!(
        budget_exceeded,
        "third tool call should be denied by budget"
    );
}

#[test]
fn truncate_output_string() {
    let value = Value::String("x".repeat(200));
    let result = helpers::truncate_output(value, 10);
    let s = result.as_str().unwrap();
    assert!(s.contains("[output truncated]"));
    assert!(s.len() < 200);
}

#[test]
fn truncate_output_json_object() {
    let value = json!({"data": "y".repeat(200)});
    let result = helpers::truncate_output(value, 10);
    let s = result.as_str().unwrap();
    assert!(s.contains("[output truncated]"));
}

#[test]
fn truncate_output_small_passes_through() {
    let value = json!("hello");
    let result = helpers::truncate_output(value.clone(), 100);
    assert_eq!(result, value);
}

#[test]
fn truncate_output_multibyte_utf8_safe() {
    // 4-byte emoji repeated — the byte boundary must not land in
    // the middle of a character.
    let emoji = "🦀".repeat(20); // 80 bytes
    let value = Value::String(emoji);
    let result = helpers::truncate_output(value, 5);
    let s = result.as_str().unwrap();
    assert!(s.contains("[output truncated]"));
    assert!(crate::tokenizer::count_tokens(s) <= 5);

    // Mix of 1-byte and 3-byte chars: "aé" is 3 bytes
    let mixed = "aé".repeat(30); // 90 bytes
    let value2 = Value::String(mixed);
    let result2 = helpers::truncate_output(value2, 2);
    let s2 = result2.as_str().unwrap();
    assert!(s2.contains("[output truncated]"));
}

#[tokio::test]
async fn respond_approval_without_pending_returns_error() {
    let model = Arc::new(ToolCallModelAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    // Drain events — no approval-requiring tools, so no pending
    while rx.recv().await.is_some() {}

    // Sending approval when nothing is pending should fail
    let result = handle.respond_approval(handle.run_id, true).await;
    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("no pending approval"),
        "should report no pending approval"
    );

    handle.wait().await;
}

#[tokio::test]
async fn register_skills_scans_and_registers_bundled_tools() {
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("greet_skill");
    fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: greet_skill
description: A greeting skill
bundled_tools:
  - name: greet
    description: Greets someone
    executable: bash
    script: scripts/greet.sh
    input_schema:
      type: object
      properties:
        name:
          type: string
---

# Greeting Skill
"#,
    )
    .unwrap();
    let script = "#!/bin/sh\nread input\necho '{\"greeting\": \"hello\"}'";
    fs::write(skill_dir.join("scripts/greet.sh"), script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            skill_dir.join("scripts/greet.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    let (tx, mut rx) = mpsc::channel(16);
    let mut registry = ToolRegistry::new();

    let result =
        skills::register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx).await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_some()); // read_file tool returned
    assert!(registry.contains("greet"));
    assert!(registry.contains("read_file"));

    // No warnings expected (no scripts/ without capabilities, since we have bundled_tools)
    drop(tx);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    // Skill has scripts/ dir but no capabilities → SkillMissingCapabilities warning
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::SkillMissingCapabilities { .. })));
}

#[tokio::test]
async fn register_skills_filters_by_allowed_skills() {
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    // Create two skills
    for name in &["skill_a", "skill_b"] {
        let skill_dir = tmp.path().join(name);
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: {name}
bundled_tools:
  - name: tool_{name}
    description: tool for {name}
    executable: bash
    script: scripts/run.sh
---
"#
            ),
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/run.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
    }

    let (tx, _rx) = mpsc::channel(16);
    let mut registry = ToolRegistry::new();
    let allowed = Some(vec!["skill_a".to_string()]);

    skills::register_skills(tmp.path().to_str().unwrap(), &allowed, &mut registry, &tx)
        .await
        .unwrap();

    assert!(registry.contains("tool_skill_a"));
    assert!(!registry.contains("tool_skill_b"));
}

#[tokio::test]
async fn register_skills_duplicate_tool_name_errors() {
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    // Create two skills with the same bundled tool name
    for name in &["skill_x", "skill_y"] {
        let skill_dir = tmp.path().join(name);
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: {name}
bundled_tools:
  - name: same_tool
    description: duplicated tool
    executable: bash
    script: scripts/run.sh
---
"#
            ),
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/run.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
    }

    let (tx, _rx) = mpsc::channel(16);
    let mut registry = ToolRegistry::new();

    let result =
        skills::register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("duplicate tool name"));
}

#[tokio::test]
async fn register_skills_no_warning_when_capabilities_declared() {
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("cap_skill");
    fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: cap_skill
description: A skill with capabilities
capabilities:
  network: true
bundled_tools:
  - name: cap_tool
    description: tool
    executable: bash
    script: scripts/run.sh
---
"#,
    )
    .unwrap();
    fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            skill_dir.join("scripts/run.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    let (tx, mut rx) = mpsc::channel(16);
    let mut registry = ToolRegistry::new();

    skills::register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx)
        .await
        .unwrap();

    drop(tx);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    // No SkillMissingCapabilities expected because capabilities are declared
    assert!(!events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::SkillMissingCapabilities { .. })));
}

#[tokio::test]
async fn register_skills_emits_missing_capabilities_warning() {
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("no_cap_skill");
    fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: no_cap_skill
description: A skill WITHOUT capabilities declared
bundled_tools:
  - name: no_cap_tool
    description: tool
    executable: bash
    script: scripts/run.sh
---
"#,
    )
    .unwrap();
    fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho '{}'").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            skill_dir.join("scripts/run.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    let (tx, mut rx) = mpsc::channel(16);
    let mut registry = ToolRegistry::new();

    skills::register_skills(tmp.path().to_str().unwrap(), &None, &mut registry, &tx)
        .await
        .unwrap();

    drop(tx);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    // Should emit SkillMissingCapabilities because no capabilities
    // section is declared in the manifest.
    assert!(
            events.iter().any(
                |e| matches!(e, RuntimeEvent::SkillMissingCapabilities { skill_name, .. } if skill_name == "no_cap_skill")
            ),
            "expected SkillMissingCapabilities for no_cap_skill"
        );
}

#[tokio::test]
async fn skills_dir_config_runs_skill_scan() {
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("run_skill");
    fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: run_skill
description: A skill
bundled_tools:
  - name: run_tool
    description: runs
    executable: bash
    script: scripts/run.sh
    input_schema:
      type: object
---
"#,
    )
    .unwrap();
    fs::write(
        skill_dir.join("scripts/run.sh"),
        "#!/bin/sh\nread input\necho '{\"result\": \"ok\"}'",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            skill_dir.join("scripts/run.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    // Use a model that calls run_tool then gives final answer
    struct SkillToolModel {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl ModelAdapter for SkillToolModel {
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
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let usage = crate::model::TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                ..Default::default()
            };
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(ModelStreamChunk::Done {
                        usage: usage.clone(),
                    })
                    .await;
            }
            if count == 0 {
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "run_tool".into(),
                        input: json!({}),
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

    let mut config = test_config();
    config.skills.dir = Some(tmp.path().to_str().unwrap().to_string());

    let model = Arc::new(SkillToolModel {
        call_count: std::sync::atomic::AtomicU32::new(0),
    });
    let registry = ToolRegistry::new();

    let (handle, mut rx) = AgentRun::start(config, "test".into(), model, registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    // Should have ToolCallCompleted for run_tool
    let tool_completed = events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "run_tool"));
    assert!(tool_completed, "run_tool should execute via skill loading");
}

// ── Sub-agent approval routing tests ──────────────────────────

/// Model shared by parent and child.  The parent calls `spawn_sub`
/// (an AgentAsTool that sends "child with approval" as the child prompt).
/// The child sees that text in its first user message and calls
/// `write_file` (which requires approval).
struct SubAgentApprovalModel;

#[async_trait::async_trait]
impl ModelAdapter for SubAgentApprovalModel {
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
            input_tokens: 2,
            output_tokens: 3,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let is_child = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::Text(t) if t.contains("child with approval")))
        });

        let has_tool_result = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });

        if has_tool_result {
            let text = if is_child {
                "child done"
            } else {
                "parent done"
            };
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(text.into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else if is_child {
            // Child calls write_file which requires approval
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "wf".into(),
                    name: "write_file".into(),
                    input: json!({"path": "test.txt"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            // Parent calls spawn_sub
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "sub".into(),
                    name: "spawn_sub".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

fn make_spawn_sub_tool() -> Arc<dyn Tool> {
    let mut child_registry = ToolRegistry::new();
    child_registry
        .register(Arc::new(FakeTool::guarded("write_file")))
        .unwrap();
    let mut config = test_config();
    config.budget.max_tokens = Some(50);
    config.budget.max_tool_calls = Some(5);
    config.budget.max_duration = Some(Duration::from_secs(10));
    config.as_tool(
        "spawn_sub",
        "spawn a sub-agent",
        Arc::new(SubAgentApprovalModel),
        child_registry,
        Arc::new(|_| Ok("child with approval".into())),
        Arc::new(|details| details.get("output").cloned().unwrap_or(details.clone())),
    )
}

#[tokio::test]
async fn sub_agent_approval_routed_to_child() {
    let model = Arc::new(SubAgentApprovalModel);
    let mut registry = ToolRegistry::new();
    registry.register(make_spawn_sub_tool()).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);

    let mut events = Vec::new();
    let mut child_approval_granted = false;
    let mut child_tool_completed = false;

    loop {
        match rx.recv().await {
            Some(RuntimeEvent::ChildRunEvent {
                child_run_id,
                event,
                ..
            })
            | Some(RuntimeEvent::SubAgentEvent {
                child_run_id,
                event,
                ..
            }) => {
                if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                    // Route approval to the child run
                    handle
                        .respond_approval(child_run_id, true)
                        .await
                        .expect("approval routing to child should succeed");
                }
                if matches!(event.as_ref(), RuntimeEvent::ApprovalGranted { .. }) {
                    child_approval_granted = true;
                }
                if matches!(event.as_ref(), RuntimeEvent::ToolCallCompleted { .. }) {
                    child_tool_completed = true;
                }
                events.push(RuntimeEvent::SubAgentEvent {
                    parent_run_id: handle.run_id,
                    child_run_id,
                    event,
                });
            }
            Some(event) => events.push(event),
            None => break,
        }
    }
    handle.wait().await;

    assert!(child_approval_granted, "child approval should be granted");
    assert!(
        child_tool_completed,
        "child tool should complete after approval"
    );
    // Should also have SubAgentCompleted
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::SubAgentCompleted { .. })));
}

#[tokio::test]
async fn sub_agent_approval_denied_completes_child() {
    let model = Arc::new(SubAgentApprovalModel);
    let mut registry = ToolRegistry::new();
    registry.register(make_spawn_sub_tool()).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);

    let mut events = Vec::new();
    let mut child_approval_denied = false;

    loop {
        match rx.recv().await {
            Some(RuntimeEvent::ChildRunEvent {
                child_run_id,
                event,
                ..
            })
            | Some(RuntimeEvent::SubAgentEvent {
                child_run_id,
                event,
                ..
            }) => {
                if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                    // Deny the approval
                    handle
                        .respond_approval(child_run_id, false)
                        .await
                        .expect("approval routing to child should succeed");
                }
                if matches!(event.as_ref(), RuntimeEvent::ApprovalDenied { .. }) {
                    child_approval_denied = true;
                }
                events.push(RuntimeEvent::SubAgentEvent {
                    parent_run_id: handle.run_id,
                    child_run_id,
                    event,
                });
            }
            Some(event) => events.push(event),
            None => break,
        }
    }
    handle.wait().await;

    assert!(
        child_approval_denied,
        "child approval should have been denied"
    );
    // Parent should still complete (SubAgentCompleted or SubAgentFailed)
    let parent_completed = events.iter().any(|e| {
        matches!(e, RuntimeEvent::SubAgentCompleted { .. })
            || matches!(e, RuntimeEvent::SubAgentFailed { .. })
    });
    assert!(
        parent_completed,
        "parent should complete after child denial"
    );
}

#[tokio::test]
async fn respond_approval_unknown_run_id_returns_error() {
    let model = Arc::new(ToolCallModelAdapter);
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FakeTool::echo())).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "hi".into(), model, registry);

    // Drain events so the run completes
    while rx.recv().await.is_some() {}

    let unknown_id = RunId::new();
    let result = handle.respond_approval(unknown_id, true).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("no pending approval"),
        "expected error about pending approval, got: {err}"
    );

    handle.wait().await;
}

#[tokio::test]
async fn child_run_events_carry_run_depth_and_child_id() {
    let model = Arc::new(SubAgentApprovalModel);
    let mut registry = ToolRegistry::new();
    registry.register(make_spawn_sub_tool()).unwrap();

    let (handle, mut rx) = AgentRun::start(test_config(), "go".into(), model, registry);
    let parent_run_id = handle.run_id;

    let mut child_run_ids = Vec::new();
    let mut child_depths = Vec::new();

    loop {
        match rx.recv().await {
            Some(RuntimeEvent::ChildRunEvent {
                child_run_id,
                run_depth,
                event,
                ..
            }) => {
                if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                    handle.respond_approval(child_run_id, true).await.unwrap();
                }
                child_run_ids.push(child_run_id);
                child_depths.push(run_depth);
            }
            Some(RuntimeEvent::SubAgentEvent {
                child_run_id,
                event,
                ..
            }) => {
                if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
                    handle.respond_approval(child_run_id, true).await.unwrap();
                }
                child_run_ids.push(child_run_id);
            }
            Some(_) => {}
            None => break,
        }
    }
    handle.wait().await;

    assert!(!child_run_ids.is_empty(), "should have child run events");
    // All child events should carry the same child_run_id
    let first_id = child_run_ids[0];
    assert!(
        child_run_ids.iter().all(|id| *id == first_id),
        "all child events should have same child_run_id"
    );
    // run_depth should be 1 (parent is depth 0)
    assert!(
        child_depths.iter().all(|d| *d == 1),
        "child run_depth should be 1"
    );
    // child_run_id should differ from parent run_id
    assert_ne!(
        first_id, parent_run_id,
        "child_run_id must differ from parent run_id"
    );
}

// ── Hook framework tests ──────────────────────────────────────────────────────

use crate::hook::{
    CompactHookContext, HandoffHookContext, Hook, HookAction, ModelHookAction, ModelHookContext,
    RunHookContext, ToolHookContext,
};
use std::sync::Mutex;

/// Records every hook invocation so tests can assert call order / count.
struct RecordingHook {
    label: &'static str,
    log: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl Hook for RecordingHook {
    async fn on_run_start(&self, _ctx: &mut RunHookContext) {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:on_run_start", self.label));
    }
    async fn on_run_end(&self, _ctx: &RunHookContext) {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:on_run_end", self.label));
    }
    async fn on_run_error(&self, _ctx: &RunHookContext, _error: &str) {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:on_run_error", self.label));
    }
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:before_model", self.label));
        ModelHookAction::Continue
    }
    async fn after_model(&self, _ctx: &mut ModelHookContext) -> HookAction {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:after_model", self.label));
        HookAction::Continue
    }
    async fn before_tool(&self, _ctx: &mut ToolHookContext) -> HookAction {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:before_tool", self.label));
        HookAction::Continue
    }
    async fn after_tool(&self, _ctx: &mut ToolHookContext) -> HookAction {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:after_tool", self.label));
        HookAction::Continue
    }
    async fn on_handoff(&self, _ctx: &HandoffHookContext) {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:on_handoff", self.label));
    }
    async fn before_compact(&self, _ctx: &mut CompactHookContext) -> HookAction {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:before_compact", self.label));
        HookAction::Continue
    }
}

/// Test A – two hooks are called in registration order.
#[tokio::test]
async fn hooks_called_in_order_and_lifecycle_events_recorded() {
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let hook_a = Arc::new(RecordingHook {
        label: "A",
        log: Arc::clone(&log),
    });
    let hook_b = Arc::new(RecordingHook {
        label: "B",
        log: Arc::clone(&log),
    });

    let mut config = test_config();
    config.hooks.push(hook_a);
    config.hooks.push(hook_b);

    let model = Arc::new(FakeModelAdapter::final_answer());
    let registry = ToolRegistry::new();

    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, registry);
    while rx.recv().await.is_some() {}
    handle.wait().await;

    let log = log.lock().unwrap();
    // Both hooks must have fired on_run_start in order A then B
    let start_a = log.iter().position(|s| s == "A:on_run_start").unwrap();
    let start_b = log.iter().position(|s| s == "B:on_run_start").unwrap();
    assert!(start_a < start_b, "A should fire before B");

    // Both hooks must have fired on_run_end
    assert!(log.iter().any(|s| s == "A:on_run_end"));
    assert!(log.iter().any(|s| s == "B:on_run_end"));

    // before_model and after_model must appear
    assert!(log.iter().any(|s| s == "A:before_model"));
    assert!(log.iter().any(|s| s == "B:before_model"));
    assert!(log.iter().any(|s| s == "A:after_model"));
    assert!(log.iter().any(|s| s == "B:after_model"));
}

/// Aborts the run from `before_model`.
struct AbortBeforeModelHook;

#[async_trait::async_trait]
impl Hook for AbortBeforeModelHook {
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        ModelHookAction::Abort("abort-reason".into())
    }
}

/// Second hook to verify it is NOT reached after an Abort.
struct WitnessHook {
    called: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Hook for WitnessHook {
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        self.called.store(true, Ordering::SeqCst);
        ModelHookAction::Continue
    }
}

use std::sync::atomic::AtomicBool;

/// Test B – `before_model` returning Abort stops the run; second hook not called.
#[tokio::test]
async fn hook_abort_before_model_stops_run_and_skips_subsequent_hooks() {
    let second_called = Arc::new(AtomicBool::new(false));

    let mut config = test_config();
    config.hooks.push(Arc::new(AbortBeforeModelHook));
    config.hooks.push(Arc::new(WitnessHook {
        called: Arc::clone(&second_called),
    }));

    let model = Arc::new(FakeModelAdapter::final_answer());
    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, ToolRegistry::new());

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunFailed { error } if error == "abort-reason")),
        "expected RunFailed with abort-reason"
    );
    assert!(
        !second_called.load(Ordering::SeqCst),
        "second hook must not be called after Abort"
    );
}

/// Returns `HookAction::Skip` from `before_tool`, preventing tool execution.
struct SkipToolHook;

#[async_trait::async_trait]
impl Hook for SkipToolHook {
    async fn before_tool(&self, _ctx: &mut ToolHookContext) -> HookAction {
        HookAction::Skip
    }
}

/// A tool that panics if executed — used to assert skip actually skips execution.
struct MustNotRunTool;

#[async_trait::async_trait]
impl Tool for MustNotRunTool {
    fn name(&self) -> &str {
        "must_not_run"
    }
    fn description(&self) -> &str {
        "panics on execute"
    }
    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
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
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        panic!("must_not_run was executed");
    }
}

/// Model that calls `must_not_run` once then returns end-turn.
struct SkipToolModel;

#[async_trait::async_trait]
impl ModelAdapter for SkipToolModel {
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
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let has_tool_result = messages
            .iter()
            .flat_map(|m| &m.content)
            .any(|b| matches!(b, ContentBlock::ToolResult { .. }));
        let usage = TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            ..Default::default()
        };
        if has_tool_result {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "must_not_run".into(),
                    input: json!({}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

/// Test C – `before_tool` Skip prevents execution; run still completes.
#[tokio::test]
async fn hook_skip_before_tool_prevents_execution_run_completes() {
    let mut config = test_config();
    config.hooks.push(Arc::new(SkipToolHook));

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(MustNotRunTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(config, "go".into(), Arc::new(SkipToolModel), registry);

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run should complete even when tool is skipped"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunFailed { .. })),
        "run must not fail"
    );
}

/// Panics inside `before_model`.
struct PanickingHook;

#[async_trait::async_trait]
impl Hook for PanickingHook {
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        panic!("deliberate hook panic");
    }
}

/// Test D – a panicking hook emits `HookPanicked` and run continues.
#[tokio::test]
async fn hook_panic_emits_event_and_run_continues() {
    let mut config = test_config();
    config.hooks.push(Arc::new(PanickingHook));

    let model = Arc::new(FakeModelAdapter::final_answer());
    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, ToolRegistry::new());

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::HookPanicked { hook_name, .. } if hook_name == "before_model")),
        "expected HookPanicked event for before_model"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run should still complete after hook panic"
    );
}

// ---------------------------------------------------------------------------
// Retry tests
// ---------------------------------------------------------------------------

/// Returns 429 for the first N calls, then a successful final answer.
struct RetryModel {
    fail_count: u32,
    call_count: AtomicU32,
}

impl RetryModel {
    fn new(fail_count: u32) -> Self {
        Self {
            fail_count,
            call_count: AtomicU32::new(0),
        }
    }
}

#[async_trait::async_trait]
impl ModelAdapter for RetryModel {
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
        _tools: &[crate::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        if n < self.fail_count {
            return Err(ModelError {
                message: "rate limited".into(),
                code: None,
                provider: None,
                status: Some(429),
                retry_after_secs: None,
                upstream: None,
            });
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

/// Test E – a 429 that resolves on the next attempt completes successfully and
/// emits a `ModelRetry` event.
#[tokio::test]
async fn retry_on_429_succeeds_after_one_failure() {
    let mut config = test_config();
    config.retry_policy = Some(super::retry::RetryPolicy {
        max_retries: 3,
        backoff: super::retry::BackoffStrategy::Fixed(Duration::from_millis(0)),
    });

    let model = Arc::new(RetryModel::new(1));
    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, ToolRegistry::new());

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ModelRetry { attempt: 1, .. })),
        "expected ModelRetry event for attempt 1"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run should complete after retry succeeds"
    );
}

/// Test F – exhausting all retries results in `RunFailed`.
#[tokio::test]
async fn retry_exhausted_results_in_run_failed() {
    let mut config = test_config();
    config.retry_policy = Some(super::retry::RetryPolicy {
        max_retries: 2,
        backoff: super::retry::BackoffStrategy::Fixed(Duration::from_millis(0)),
    });

    // always 429 → will exhaust max_retries
    let model = Arc::new(RetryModel::new(10));
    let (handle, mut rx) = AgentRun::start(config, "hi".into(), model, ToolRegistry::new());

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    handle.wait().await;

    let retry_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::ModelRetry { .. }))
        .collect();
    assert_eq!(retry_events.len(), 2, "expected exactly 2 retry events");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunFailed { .. })),
        "run should fail after retries exhausted"
    );
}
