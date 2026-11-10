//! Event surface tests (v2.0 issue 007): message lifecycle, request and
//! ledger events.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::checkpoint_tests::{ScriptedModel, Turn};
use super::*;
use crate::events::{MessageDelta, RuntimeEvent};
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolEffects, ToolError, ToolMetadata,
    ToolOutput,
};

/// Streams a thinking delta and two text deltas, then answers.
struct StreamingModel;

#[async_trait::async_trait]
impl ModelAdapter for StreamingModel {
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
        if let Some(tx) = tx {
            for chunk in [
                StreamEvent::Thinking {
                    delta: "hmm".into(),
                },
                StreamEvent::Text {
                    delta: "hel".into(),
                },
                StreamEvent::Text { delta: "lo".into() },
            ] {
                tx.send(chunk).await.unwrap();
            }
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("hello".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

fn config() -> AgentConfig {
    let mut config = AgentConfig::builder("events-agent", "mock")
        .system_prompt("system")
        .max_steps(10)
        .build()
        .unwrap();
    config.supervision_strategy = SupervisionStrategy::Stop;
    config
}

async fn drain(handle: RunHandle, mut rx: EventReceiver) -> Vec<RuntimeEvent> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;
    events
}

#[tokio::test]
async fn message_lifecycle_separates_text_and_thinking() {
    let (handle, rx) = AgentRun::start(
        config(),
        "hi".into(),
        Arc::new(StreamingModel),
        ToolRegistry::new(),
    );
    let events = drain(handle, rx).await;

    let started: Vec<&String> = events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::MessageStarted { message_id, .. } => Some(message_id),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 1);
    let id = started[0];
    let deltas: Vec<MessageDelta> = events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::MessageUpdated { message_id, delta } if message_id == id => {
                Some(delta.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        deltas,
        vec![
            MessageDelta::Thinking { text: "hmm".into() },
            MessageDelta::Text { text: "hel".into() },
            MessageDelta::Text { text: "lo".into() },
        ]
    );
    let ended = events.iter().find_map(|e| match e {
        RuntimeEvent::MessageEnded {
            message_id,
            content,
            stop_reason,
        } if message_id == id => Some((content.clone(), stop_reason.clone())),
        _ => None,
    });
    assert_eq!(
        ended,
        Some((
            vec![ContentBlock::Text("hello".into())],
            Some(StopReason::EndTurn)
        ))
    );
    let position = |pred: &dyn Fn(&RuntimeEvent) -> bool| events.iter().position(pred).unwrap();
    assert!(
        position(&|e| matches!(e, RuntimeEvent::MessageStarted { .. }))
            < position(&|e| matches!(e, RuntimeEvent::MessageEnded { .. }))
    );
}

struct EchoTool {
    metadata: ToolMetadata,
}

#[async_trait::async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "read"
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
        &self.metadata
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(input))
    }
}

#[tokio::test]
async fn ledger_events_follow_dispatch_and_settlement() {
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(EchoTool {
            metadata: ToolMetadata {
                effects: ToolEffects::READ_ONLY,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
        }))
        .unwrap();
    let model = ScriptedModel::new(vec![
        Turn::CallTool {
            id: "r1",
            name: "read",
        },
        Turn::CallTool {
            id: "m1",
            name: "missing",
        },
        Turn::Answer,
    ]);
    let (handle, rx) = AgentRun::start(config(), "go".into(), model, registry);
    let events = drain(handle, rx).await;

    let ledger: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::ToolDispatched { tool_use_id, .. } => {
                Some(format!("dispatched {tool_use_id}"))
            }
            RuntimeEvent::ToolSettled {
                tool_use_id,
                is_error,
                error_code,
            } => Some(format!(
                "settled {tool_use_id} {is_error} {}",
                error_code.as_deref().unwrap_or("-")
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        ledger,
        vec![
            "dispatched r1",
            "settled r1 false -",
            // An unknown tool is never dispatched but still settles.
            "settled m1 true NOT_FOUND",
        ]
    );
}

/// A tool that asks the host a question of its own kind.
struct AskTool {
    metadata: ToolMetadata,
}

#[async_trait::async_trait]
impl Tool for AskTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "asks"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }
    async fn execute(&self, _input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let resolution = ctx
            .request(
                self.name(),
                RequestKind::new("plan_review"),
                json!({"plan": ["a", "b"]}),
                Duration::from_secs(5),
            )
            .await;
        match resolution {
            RequestResolution::Responded {
                response:
                    RequestResponse {
                        decision: RequestDecision::Answer { payload },
                        ..
                    },
            } => Ok(ToolOutput::Immediate(payload)),
            other => Ok(ToolOutput::Immediate(
                json!({ "unanswered": format!("{other:?}") }),
            )),
        }
    }
}

#[tokio::test]
async fn tools_raise_host_defined_requests_on_the_same_channel() {
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(AskTool {
            metadata: ToolMetadata {
                effects: ToolEffects::NONE,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
        }))
        .unwrap();
    let model = ScriptedModel::new(vec![Turn::CallTool {
        id: "a1",
        name: "read",
    }]);
    let (handle, mut rx) = AgentRun::start(config(), "go".into(), model.clone(), registry);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        if let RuntimeEvent::RequestOpened { request } = &event {
            assert_eq!(request.kind.as_str(), "plan_review");
            assert_eq!(request.payload, json!({"plan": ["a", "b"]}));
            assert!(handle.request(&request.id).await.is_some());
            handle
                .respond(&request.id, RequestResponse::answer(json!("accept a")))
                .await
                .unwrap();
        }
        events.push(event);
    }
    handle.wait().await;

    assert!(events.iter().any(|e| matches!(
        e,
        RuntimeEvent::RequestResolved { kind, resolution, .. }
            if kind.as_str() == "plan_review" && resolution.is_approved()
    )));
    let last = model.seen().pop().unwrap();
    assert!(last.iter().flat_map(|m| &m.content).any(|b| matches!(
        b,
        ContentBlock::ToolResult { tool_use_id, content }
            if tool_use_id == "a1" && content == &json!("accept a")
    )));
}

#[tokio::test]
async fn approvals_are_reported_as_generic_requests_too() {
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(EchoTool {
            metadata: ToolMetadata {
                effects: ToolEffects::UNKNOWN,
                approval: Approval::Always,
                ..ToolMetadata::default()
            },
        }))
        .unwrap();
    let model = ScriptedModel::new(vec![Turn::CallTool {
        id: "r1",
        name: "read",
    }]);
    let (handle, mut rx) = AgentRun::start(config(), "go".into(), model, registry);
    let mut opened = Vec::new();
    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::RequestOpened { request } => opened.push(request.clone()),
            RuntimeEvent::ApprovalRequested { request_id, .. } => {
                assert_eq!(opened.last().map(|r| &r.id), Some(request_id));
                handle
                    .respond(request_id, RequestResponse::approve())
                    .await
                    .unwrap();
            }
            _ => {}
        }
    }
    handle.wait().await;
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].kind.as_str(), RequestKind::TOOL_APPROVAL);
}

#[test]
fn run_handle_reports_capabilities() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (handle, rx) = AgentRun::start(
            config(),
            "hi".into(),
            Arc::new(StreamingModel),
            ToolRegistry::new(),
        );
        let caps = handle.capabilities().clone();
        drain(handle, rx).await;
        assert!(caps.preemptive_cancel);
        assert!(!caps.resume);
        assert_eq!(caps, config().capabilities());
    });
}
