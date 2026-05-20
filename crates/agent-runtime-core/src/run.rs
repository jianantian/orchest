use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::budget::{BudgetConfig, BudgetGuard, BudgetUsage};
use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelResponse, ModelSpec, ModelStreamChunk, Role,
    StopReason,
};
use crate::tool::async_job::{JobHandle, JobStatus};
use crate::tool::registry::ToolRegistry;
use crate::tool::{Tool, ToolCall, ToolContext, ToolOutput};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: String,
    pub model: ModelSpec,
    pub budget: BudgetConfig,
    pub max_steps: u32,
    pub allowed_skills: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub mcp_servers: Vec<Value>,
}

pub struct RunState {
    pub run_id: RunId,
    pub schema_version: String,
    pub config: AgentConfig,
    pub messages: Vec<Message>,
    pub available_tools: Vec<Arc<dyn Tool>>,
    pub step: u32,
    pub status: RunStatus,
    pub budget_used: BudgetUsage,
}

#[derive(Debug)]
pub enum RunStatus {
    Running,
    WaitingForApproval {
        tool_call: ToolCall,
    },
    WaitingForAsyncTool {
        tool_call: ToolCall,
        job_handle: JobHandle,
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

pub type EventReceiver = mpsc::Receiver<RuntimeEvent>;

pub struct RunHandle {
    pub run_id: RunId,
    task: tokio::task::JoinHandle<()>,
    approval_tx: mpsc::Sender<bool>,
}

impl RunHandle {
    pub async fn wait(self) {
        let _ = self.task.await;
    }

    pub async fn respond_approval(&self, _run_id: RunId, approved: bool) {
        let _ = self.approval_tx.send(approved).await;
    }
}

pub struct AgentRun;

impl AgentRun {
    pub fn start(
        config: AgentConfig,
        input: String,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
    ) -> (RunHandle, EventReceiver) {
        let run_id = RunId::new();
        let (event_tx, event_rx) = mpsc::channel(256);
        let (approval_tx, approval_rx) = mpsc::channel(1);

        let task = tokio::spawn(async move {
            run_loop(
                run_id,
                config,
                input,
                model,
                registry,
                event_tx,
                approval_rx,
            )
            .await;
        });

        let handle = RunHandle {
            run_id,
            task,
            approval_tx,
        };
        (handle, event_rx)
    }
}

async fn emit(tx: &mpsc::Sender<RuntimeEvent>, event: RuntimeEvent) {
    let _ = tx.send(event).await;
}

async fn run_loop(
    run_id: RunId,
    config: AgentConfig,
    input: String,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    tx: mpsc::Sender<RuntimeEvent>,
    mut approval_rx: mpsc::Receiver<bool>,
) {
    emit(&tx, RuntimeEvent::RunStarted { run_id }).await;

    let mut messages = vec![
        Message {
            role: Role::System,
            content: vec![ContentBlock::Text(config.system_prompt.clone())],
        },
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text(input)],
        },
    ];

    let tool_defs = registry.list();
    let mut step: u32 = 0;
    let mut budget = BudgetGuard::new(config.budget.clone());

    loop {
        if step >= config.max_steps {
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: "max_steps_reached".into(),
                },
            )
            .await;
            return;
        }

        if let Some(_violation) = budget.check() {
            emit(
                &tx,
                RuntimeEvent::BudgetWarning {
                    used: budget.usage().clone(),
                    limit: budget.config().clone(),
                },
            )
            .await;
            emit(
                &tx,
                RuntimeEvent::RunFailed {
                    error: "budget_exceeded".into(),
                },
            )
            .await;
            return;
        }

        emit(&tx, RuntimeEvent::ModelCallStarted { step }).await;

        let (stream_tx, mut stream_rx) = mpsc::channel::<ModelStreamChunk>(64);
        let event_tx_clone = tx.clone();
        let forward_task = tokio::spawn(async move {
            while let Some(chunk) = stream_rx.recv().await {
                let _ = event_tx_clone
                    .send(RuntimeEvent::ModelStreamChunk { delta: chunk })
                    .await;
            }
        });

        let response = model.stream(&messages, &tool_defs, stream_tx).await;
        let _ = forward_task.await;

        let response: ModelResponse = match response {
            Ok(r) => r,
            Err(e) => {
                emit(
                    &tx,
                    RuntimeEvent::RunFailed {
                        error: e.to_string(),
                    },
                )
                .await;
                return;
            }
        };

        budget.record_model_call(&response.usage);

        emit(
            &tx,
            RuntimeEvent::ModelCallCompleted {
                tokens: response.usage.clone(),
            },
        )
        .await;

        let mut tool_uses = Vec::new();
        let mut text_parts = Vec::new();

        for block in &response.content {
            match block {
                ContentBlock::ToolUse { id, name, input } => {
                    tool_uses.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                }
                ContentBlock::Text(t) => {
                    text_parts.push(t.clone());
                }
                _ => {}
            }
        }

        match response.stop_reason {
            StopReason::EndTurn if tool_uses.is_empty() => {
                let output = json!(text_parts.join(""));
                emit(&tx, RuntimeEvent::RunCompleted { output }).await;
                return;
            }
            StopReason::MaxTokens if tool_uses.is_empty() => {
                let output = json!(text_parts.join(""));
                emit(&tx, RuntimeEvent::RunCompleted { output }).await;
                return;
            }
            _ => {}
        }

        messages.push(Message {
            role: Role::Assistant,
            content: response.content.clone(),
        });

        let mut tool_results = Vec::new();

        for tool_call in &tool_uses {
            let tool = match registry.get(&tool_call.name) {
                Some(t) => t,
                None => {
                    emit(
                        &tx,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: format!("tool '{}' not found", tool_call.name),
                        },
                    )
                    .await;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": format!("tool '{}' not found", tool_call.name)}),
                    });
                    continue;
                }
            };

            if tool.metadata().requires_approval {
                emit(
                    &tx,
                    RuntimeEvent::ApprovalRequested {
                        tool_call: tool_call.clone(),
                    },
                )
                .await;

                let approved = approval_rx.recv().await.unwrap_or(false);

                if approved {
                    emit(
                        &tx,
                        RuntimeEvent::ApprovalGranted {
                            tool_call: tool_call.clone(),
                        },
                    )
                    .await;
                } else {
                    emit(
                        &tx,
                        RuntimeEvent::ApprovalDenied {
                            tool_call: tool_call.clone(),
                        },
                    )
                    .await;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": "tool call denied by user"}),
                    });
                    continue;
                }
            }

            let source = tool.metadata().source.clone();
            emit(
                &tx,
                RuntimeEvent::ToolCallStarted {
                    tool: tool_call.name.clone(),
                    source,
                    input: tool_call.input.clone(),
                },
            )
            .await;

            let ctx = ToolContext {
                run_id,
                tool_call_id: tool_call.id.clone(),
                on_update: None,
            };

            let start_time = Instant::now();
            let result = tool.execute(tool_call.input.clone(), &ctx).await;

            match result {
                Ok(ToolOutput::Immediate(value)) => {
                    let duration = start_time.elapsed();
                    emit(
                        &tx,
                        RuntimeEvent::ToolCallCompleted {
                            tool: tool_call.name.clone(),
                            output: value.clone(),
                            duration,
                        },
                    )
                    .await;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: value,
                    });
                }
                Ok(ToolOutput::AsyncJob(handle)) => {
                    emit(
                        &tx,
                        RuntimeEvent::AsyncToolStarted {
                            tool: tool_call.name.clone(),
                            job_id: handle.job_id.clone(),
                        },
                    )
                    .await;

                    let async_result =
                        poll_async_job(&tx, &tool_call.name, &handle, start_time).await;

                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: async_result,
                    });
                }
                Err(e) => {
                    emit(
                        &tx,
                        RuntimeEvent::ToolCallFailed {
                            tool: tool_call.name.clone(),
                            error: e.message.clone(),
                        },
                    )
                    .await;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: json!({"error": e.message}),
                    });
                }
            }

            budget.record_tool_call();
        }

        messages.push(Message {
            role: Role::User,
            content: tool_results,
        });

        step += 1;
    }
}

async fn poll_async_job(
    tx: &mpsc::Sender<RuntimeEvent>,
    tool_name: &str,
    handle: &JobHandle,
    start_time: Instant,
) -> Value {
    let timeout = handle.timeout;

    loop {
        tokio::time::sleep(handle.poll_interval).await;

        if let Some(max) = timeout {
            if start_time.elapsed() > max {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: "async job timed out".into(),
                    },
                )
                .await;
                return json!({"error": "async job timed out"});
            }
        }

        match (handle.poll)().await {
            Ok(JobStatus::Pending { progress, message }) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolProgress {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        status: JobStatus::Pending { progress, message },
                    },
                )
                .await;
            }
            Ok(JobStatus::Completed(value)) => {
                let elapsed = start_time.elapsed();
                emit(
                    tx,
                    RuntimeEvent::AsyncToolCompleted {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        output: value.clone(),
                        elapsed,
                    },
                )
                .await;
                return value;
            }
            Ok(JobStatus::Failed(err)) => {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: err.clone(),
                    },
                )
                .await;
                return json!({"error": err});
            }
            Err(e) => {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: e.message.clone(),
                    },
                )
                .await;
                return json!({"error": e.message});
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::BudgetConfig;
    use crate::model::{ModelError, TokenUsage};
    use crate::tool::{JsonSchema, ToolDef, ToolError, ToolMetadata, ToolSource};
    use std::sync::atomic::{AtomicU32, Ordering};

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
        async fn stream(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            tx: mpsc::Sender<ModelStreamChunk>,
        ) -> Result<ModelResponse, ModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            let _ = tx
                .send(ModelStreamChunk::Text {
                    delta: "hello".into(),
                })
                .await;
            let usage = TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            };
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;

            if count == 0 {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("hello".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                })
            } else {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
                })
            }
        }
    }

    fn test_config() -> AgentConfig {
        AgentConfig {
            system_prompt: "you are helpful".into(),
            model: ModelSpec {
                provider: "test".into(),
                model: "test".into(),
                api_key_env: None,
                max_tokens: None,
            },
            budget: BudgetConfig {
                max_tokens: None,
                max_tool_calls: None,
                max_duration: None,
                max_cost_usd: None,
            },
            max_steps: 10,
            allowed_skills: None,
            allowed_tools: None,
            mcp_servers: vec![],
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
        config.max_steps = 0;

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
        async fn stream(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            tx: mpsc::Sender<ModelStreamChunk>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });

            let usage = TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            };
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;

            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
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

    struct ApprovalModelAdapter;

    #[async_trait::async_trait]
    impl ModelAdapter for ApprovalModelAdapter {
        async fn stream(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            tx: mpsc::Sender<ModelStreamChunk>,
        ) -> Result<ModelResponse, ModelError> {
            let has_tool_result = messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            });
            let usage = TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
            };
            let _ = tx
                .send(ModelStreamChunk::Done {
                    usage: usage.clone(),
                })
                .await;

            if has_tool_result {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("done".into())],
                    usage,
                    stop_reason: StopReason::EndTurn,
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
                    handle.respond_approval(handle.run_id, true).await;
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
                    handle.respond_approval(handle.run_id, false).await;
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
}
