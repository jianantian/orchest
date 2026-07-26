//! Integration tests for v0.7 features: retry, loop detection, handoff, hooks, agent-as-tool.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::handoff::{Handoff, HandoffTarget};
use orchest::hook::{Hook, LoopDetectionConfig, ModelHookAction, ModelHookContext};
use orchest::model::{
    ContentBlock, JsonSchema, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun, BackoffStrategy, RetryPolicy};
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ── Shared mock helpers ───────────────────────────────────────────────────────

fn ok_response(text: &str) -> ModelResponse {
    ModelResponse {
        content: vec![ContentBlock::Text(text.into())],
        usage: TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
        option_adjustments: vec![],
    }
}

fn rate_limit_error() -> ModelError {
    ModelError {
        message: "rate limit".into(),
        code: Some("rate_limit_exceeded".into()),
        provider: Some("mock".into()),
        status: Some(429),
        retry_after_secs: None,
        upstream: None,
    }
}

async fn collect_events(mut rx: tokio::sync::mpsc::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    events
}

// ── Test 1: retry policy fires ModelRetry events then RunFailed ───────────────

struct AlwaysRateLimitModel;

#[async_trait]
impl ModelAdapter for AlwaysRateLimitModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "rate-limited"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        Err(rate_limit_error())
    }
}

#[tokio::test]
async fn retry_policy_fires_retry_events_then_run_failed() {
    let config = AgentConfig::builder("mock/rate-limited")
        .system_prompt("assistant")
        .max_steps(5)
        .retry_policy(RetryPolicy {
            max_retries: 2,
            backoff: BackoffStrategy::Fixed(Duration::ZERO),
        })
        .build()
        .unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "go".into(),
        Arc::new(AlwaysRateLimitModel),
        ToolRegistry::new(),
    );
    let events = collect_events(rx).await;
    handle.wait().await;

    let retries: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::ModelRetry { attempt, .. } => Some(*attempt),
            _ => None,
        })
        .collect();
    assert_eq!(retries, vec![1, 2], "expected two ModelRetry events");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunFailed { .. })),
        "expected RunFailed after retry exhaustion"
    );
}

// ── Test 2: loop detection aborts on repeated tool calls ──────────────────────

struct EchoSearchTool;

#[async_trait]
impl Tool for EchoSearchTool {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "search the web"
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
            source: ToolSource::Builtin,
        }
    }
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(json!({"result": "nothing"})))
    }
}

struct RepeatedToolModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for RepeatedToolModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "repeating"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 3,
            output_tokens: 1,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::ToolUse {
                id: format!("c{n}"),
                name: "search".into(),
                input: json!({"q": "same query"}),
            }],
            usage,
            stop_reason: StopReason::ToolUse,
            option_adjustments: vec![],
        })
    }
}

#[tokio::test]
async fn loop_detection_aborts_on_repeated_tool_calls() {
    let config = AgentConfig::builder("mock/repeating")
        .system_prompt("assistant")
        .max_steps(10)
        .build()
        .unwrap()
        .with_loop_detection_config(LoopDetectionConfig {
            window_size: 10,
            warn_threshold: 2,
            stop_threshold: 3,
        });

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoSearchTool)).unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "search".into(),
        Arc::new(RepeatedToolModel {
            call_count: Default::default(),
        }),
        registry,
    );
    let events = collect_events(rx).await;
    handle.wait().await;

    assert!(
        events.iter().any(|e| match e {
            RuntimeEvent::RunFailed { error } => error.contains("loop"),
            _ => false,
        }),
        "loop detection must emit RunFailed with loop message"
    );
}

// ── Test 3: hook returning Abort emits RunFailed ──────────────────────────────

struct AbortHook;

#[async_trait]
impl Hook for AbortHook {
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        ModelHookAction::Abort("hook aborted".into())
    }
}

struct NeverCalledModel;

#[async_trait]
impl ModelAdapter for NeverCalledModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "never-called"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        panic!("model must not be called when hook aborts");
    }
}

#[tokio::test]
async fn hook_abort_before_model_emits_run_failed() {
    let config = AgentConfig::builder("mock/never-called")
        .system_prompt("assistant")
        .max_steps(2)
        .build()
        .unwrap()
        .with_hook(Arc::new(AbortHook));

    let (handle, rx) = AgentRun::start(
        config,
        "go".into(),
        Arc::new(NeverCalledModel),
        ToolRegistry::new(),
    );
    let events = collect_events(rx).await;
    handle.wait().await;

    assert!(
        events.iter().any(|e| match e {
            RuntimeEvent::RunFailed { error } => error.contains("hook aborted"),
            _ => false,
        }),
        "expected RunFailed with hook abort message"
    );
}

// ── Test 4: handoff routing emits AgentUpdated ────────────────────────────────

struct HandoffRoutingModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for HandoffRoutingModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "routing"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 2,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if n == 0 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "h1".into(),
                    name: "route_to_billing".into(),
                    input: json!({"reason": "billing"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("billing response".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

#[tokio::test]
async fn handoff_routing_emits_agent_updated() {
    let billing_config = AgentConfig::builder("mock/routing")
        .system_prompt("billing agent")
        .max_steps(2)
        .build()
        .unwrap();

    let triage_config = AgentConfig::builder("mock/routing")
        .system_prompt("triage agent")
        .max_steps(4)
        .build()
        .unwrap()
        .with_handoff(Handoff {
            tool_name: "route_to_billing".into(),
            tool_description: "Route to billing".into(),
            input_schema: json!({"type": "object"}),
            target: HandoffTarget::Static(Box::new(billing_config)),
            input_filter: None,
            nest_history: false,
        });

    let model = Arc::new(HandoffRoutingModel {
        call_count: Default::default(),
    });

    let (handle, rx) = AgentRun::start(
        triage_config,
        "billing question".into(),
        model,
        ToolRegistry::new(),
    );
    let events = collect_events(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::AgentUpdated { .. })),
        "handoff must emit AgentUpdated"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run must complete after handoff"
    );
}

// ── Test 5: agent_as_tool executes child and emits SubAgent events ────────────

struct ParentAgentModel {
    call_count: AtomicU32,
}

#[async_trait]
impl ModelAdapter for ParentAgentModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "parent"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 8,
            output_tokens: 4,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if n == 0 {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "c1".into(),
                    name: "summariser".into(),
                    input: json!({"input": "text to summarise"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            let child_summary = messages
                .iter()
                .find_map(|m| {
                    m.content.iter().find_map(|b| match b {
                        ContentBlock::ToolResult { content, .. } => content
                            .get("output")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| "(no result)".into());
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(format!("parent done: {child_summary}"))],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

struct ChildAgentModel;

#[async_trait]
impl ModelAdapter for ChildAgentModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "child"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[orchest::tool::ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let usage = TokenUsage {
            input_tokens: 4,
            output_tokens: 2,
            ..Default::default()
        };
        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        Ok(ok_response("summary"))
    }
}

#[tokio::test]
async fn agent_as_tool_emits_sub_agent_events() {
    let child_config = AgentConfig::builder("mock/child")
        .system_prompt("summariser")
        .max_steps(2)
        .build()
        .unwrap();

    let summariser_tool = child_config
        .as_tool("summariser", "Summarises text")
        .model(Arc::new(ChildAgentModel))
        .registry(ToolRegistry::new())
        .input_mapper(|input: Value| {
            input
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ToolError::fatal("missing input"))
        })
        .output_extractor(|details: Value| {
            json!({"output": details.get("output").cloned().unwrap_or_else(|| details.clone())})
        })
        .build()
        .unwrap();

    let parent_config = AgentConfig::builder("mock/parent")
        .system_prompt("research assistant")
        .max_steps(3)
        .build()
        .unwrap();

    let mut parent_registry = ToolRegistry::new();
    parent_registry.register(summariser_tool).unwrap();

    let model = Arc::new(ParentAgentModel {
        call_count: Default::default(),
    });

    let (handle, rx) = AgentRun::start(
        parent_config,
        "summarise this".into(),
        model,
        parent_registry,
    );
    let events = collect_events(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SubAgentStarted { .. })),
        "expected SubAgentStarted"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SubAgentCompleted { .. })),
        "expected SubAgentCompleted"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "expected RunCompleted"
    );
    assert!(
        events.iter().any(|e| match e {
            RuntimeEvent::RunCompleted { output, .. } => output.to_string().contains("summary"),
            _ => false,
        }),
        "parent output must include the child agent's summary result"
    );
}
