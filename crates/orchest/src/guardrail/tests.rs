//! Integration tests for the guardrail adapters, driven through a real run loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::*;
use crate::budget::BudgetConfig;
use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse, ModelSpec,
    ModelStreamChunk, RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use crate::run::{config, AgentConfig, AgentRun};
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    JsonSchema, Tool, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput, ToolSource,
};

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
        handoffs: vec![],
        session_store: None,
        session_id: None,
        supervision_strategy: Default::default(),
    }
}

/// Model that calls `echo` once, then ends after seeing a tool result.
struct EchoCaller;

#[async_trait]
impl ModelAdapter for EchoCaller {
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
                    name: "echo".into(),
                    input: json!({"text": "original"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }
}

/// Single-turn model returning plain text.
struct TextModel;

#[async_trait]
impl ModelAdapter for TextModel {
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
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("secret-leak".into())],
            usage,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

/// Echo tool that records the input it received.
struct RecordingEcho {
    seen_input: Arc<std::sync::Mutex<Option<Value>>>,
}

#[async_trait]
impl Tool for RecordingEcho {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "echo"
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
            approval: crate::tool::Approval::Never,
            execution_mode: crate::tool::ToolExecutionMode::Normal,
            parallelism: crate::tool::ToolParallelism::Serial,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        *self.seen_input.lock().unwrap() = Some(input.clone());
        Ok(ToolOutput::Immediate(input))
    }
}

fn echo_registry() -> (ToolRegistry, Arc<std::sync::Mutex<Option<Value>>>) {
    let seen = Arc::new(std::sync::Mutex::new(None));
    let mut reg = ToolRegistry::new();
    reg.register(Arc::new(RecordingEcho {
        seen_input: seen.clone(),
    }))
    .unwrap();
    (reg, seen)
}

async fn drain(mut rx: mpsc::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    let mut out = Vec::new();
    while let Some(e) = rx.recv().await {
        out.push(e);
    }
    out
}

// ── tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tool_input_guardrail_modify_changes_input() {
    struct ModifyInput;
    #[async_trait]
    impl ToolInputGuardrail for ModifyInput {
        async fn check(&self, _ctx: &ToolHookContext) -> ToolInputGuardrailAction {
            ToolInputGuardrailAction::Modify(json!({"text": "modified"}))
        }
    }
    let (registry, seen) = echo_registry();
    let config = test_config().with_tool_input_guardrail(Arc::new(ModifyInput));
    let (handle, rx) = AgentRun::start(config, "hi".into(), Arc::new(EchoCaller), registry);
    drain(rx).await;
    handle.wait().await;
    assert_eq!(*seen.lock().unwrap(), Some(json!({"text": "modified"})));
}

#[tokio::test]
async fn tool_input_guardrail_reject_returns_reason() {
    struct RejectInput;
    #[async_trait]
    impl ToolInputGuardrail for RejectInput {
        async fn check(&self, _ctx: &ToolHookContext) -> ToolInputGuardrailAction {
            ToolInputGuardrailAction::Reject("forbidden".into())
        }
    }
    let (registry, seen) = echo_registry();
    let config = test_config().with_tool_input_guardrail(Arc::new(RejectInput));
    let (handle, rx) = AgentRun::start(config, "hi".into(), Arc::new(EchoCaller), registry);
    let events = drain(rx).await;
    handle.wait().await;
    // tool never executed
    assert!(seen.lock().unwrap().is_none());
    assert!(!events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallStarted { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })));
}

#[tokio::test]
async fn tool_output_guardrail_modify_changes_result() {
    struct ModifyOutput;
    #[async_trait]
    impl ToolOutputGuardrail for ModifyOutput {
        async fn check(&self, _ctx: &ToolHookContext) -> ToolOutputGuardrailAction {
            ToolOutputGuardrailAction::Modify(json!("sanitized"))
        }
    }
    let (registry, _seen) = echo_registry();
    let config = test_config().with_tool_output_guardrail(Arc::new(ModifyOutput));
    let (handle, rx) = AgentRun::start(config, "hi".into(), Arc::new(EchoCaller), registry);
    let events = drain(rx).await;
    handle.wait().await;
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ToolCallCompleted { .. })));
}

#[tokio::test]
async fn output_guardrail_replace_changes_response() {
    struct Redact;
    #[async_trait]
    impl OutputGuardrail for Redact {
        async fn check(&self, ctx: &ModelHookContext) -> OutputGuardrailAction {
            assert!(ctx.response.is_some());
            OutputGuardrailAction::Replace(vec![ContentBlock::Text("[redacted]".into())])
        }
    }
    let config = test_config().with_output_guardrail(Arc::new(Redact));
    let (handle, rx) = AgentRun::start(
        config,
        "hi".into(),
        Arc::new(TextModel),
        ToolRegistry::new(),
    );
    let events = drain(rx).await;
    handle.wait().await;
    assert!(events.iter().any(|e| matches!(
        e,
        RuntimeEvent::RunCompleted { output, .. } if output.as_str() == Some("[redacted]")
    )));
}

#[tokio::test]
async fn input_guardrail_abort_terminates_run() {
    struct AbortInput;
    #[async_trait]
    impl InputGuardrail for AbortInput {
        async fn check(&self, _ctx: &ModelHookContext) -> InputGuardrailAction {
            InputGuardrailAction::Abort("blocked input".into())
        }
    }
    let config = test_config().with_input_guardrail(Arc::new(AbortInput));
    let (handle, rx) = AgentRun::start(
        config,
        "hi".into(),
        Arc::new(TextModel),
        ToolRegistry::new(),
    );
    let events = drain(rx).await;
    handle.wait().await;
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunFailed { error } if error == "blocked input")));
}

#[tokio::test]
async fn chained_tool_input_guardrails_short_circuit() {
    let second_ran = Arc::new(AtomicBool::new(false));
    struct First;
    #[async_trait]
    impl ToolInputGuardrail for First {
        async fn check(&self, _ctx: &ToolHookContext) -> ToolInputGuardrailAction {
            ToolInputGuardrailAction::Reject("first".into())
        }
    }
    struct Second(Arc<AtomicBool>);
    #[async_trait]
    impl ToolInputGuardrail for Second {
        async fn check(&self, _ctx: &ToolHookContext) -> ToolInputGuardrailAction {
            self.0.store(true, Ordering::SeqCst);
            ToolInputGuardrailAction::Allow
        }
    }
    let (registry, _seen) = echo_registry();
    let config = test_config()
        .with_tool_input_guardrail(Arc::new(First))
        .with_tool_input_guardrail(Arc::new(Second(second_ran.clone())));
    let (handle, rx) = AgentRun::start(config, "hi".into(), Arc::new(EchoCaller), registry);
    drain(rx).await;
    handle.wait().await;
    assert!(
        !second_ran.load(Ordering::SeqCst),
        "second guardrail must not run after first rejects"
    );
}
