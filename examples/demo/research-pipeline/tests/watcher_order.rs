use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use orchest::{
    events::RuntimeEvent,
    model::{
        ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
        RequestOptions, StopReason, TokenUsage, ToolDef,
    },
    run::{AgentConfig, AgentRun},
    tool::{
        registry::ToolRegistry, Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput,
        ToolSource,
    },
};
use research_pipeline_demo::watcher::{stable_event_key, CountingWatcher};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Notify};

const ACTIVATION_STEP: u32 = 1;
const PROBE_TOOL: &str = "ordering_probe";
const ORDERED_TOOL: &str = "ordered_work";
const WATCHER_CAPACITY: usize = 1024;

#[derive(Default)]
struct CallGate {
    entered: Notify,
    release: Notify,
}

impl CallGate {
    async fn hold(&self) {
        self.entered.notify_one();
        self.release.notified().await;
    }

    async fn wait_until_entered(&self) {
        self.entered.notified().await;
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

struct OrderedSupervisorModel {
    calls: AtomicU32,
    first_call_gate: Arc<CallGate>,
}

#[async_trait]
impl ModelAdapter for OrderedSupervisorModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "ordered-supervisor"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<orchest::model::StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        match call {
            0 => {
                self.first_call_gate.hold().await;
                Ok(tool_response("probe-1", PROBE_TOOL))
            }
            1 => Ok(tool_response("ordered-1", ORDERED_TOOL)),
            _ => Ok(ModelResponse {
                content: vec![ContentBlock::Text("ordered run complete".to_string())],
                usage: TokenUsage::default(),
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
        }
    }
}

fn tool_response(id: &str, tool: &str) -> ModelResponse {
    ModelResponse {
        content: vec![ContentBlock::ToolUse {
            id: id.to_string(),
            name: tool.to_string(),
            input: json!({}),
        }],
        usage: TokenUsage::default(),
        stop_reason: StopReason::ToolUse,
        option_adjustments: vec![],
    }
}

struct NoopTool {
    name: &'static str,
    schema: Value,
    metadata: ToolMetadata,
}

impl NoopTool {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            schema: json!({"type": "object"}),
            metadata: ToolMetadata {
                source: ToolSource::InProcess,
                side_effect: false,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
        }
    }
}

#[async_trait]
impl Tool for NoopTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "Return one deterministic ordering checkpoint."
    }

    fn input_schema(&self) -> &Value {
        &self.schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(json!({"checkpoint": self.name})))
    }
}

fn is_terminal(event: &RuntimeEvent) -> bool {
    matches!(
        event,
        RuntimeEvent::RunCompleted { .. }
            | RuntimeEvent::RunFailed { .. }
            | RuntimeEvent::RunAborted { .. }
    )
}

fn assert_subsequence(sequence: &[String], milestones: &[&str]) {
    let mut next = 0;
    for key in sequence {
        if milestones
            .get(next)
            .is_some_and(|milestone| key == milestone)
        {
            next += 1;
        }
    }
    assert_eq!(
        next,
        milestones.len(),
        "missing ordered milestone {:?} from sequence {sequence:?}",
        milestones.get(next)
    );
}

#[tokio::test]
async fn two_watchers_preserve_fifo_and_match_the_complete_no_drop_sequence() {
    let first_call_gate = Arc::new(CallGate::default());
    let model: Arc<dyn ModelAdapter> = Arc::new(OrderedSupervisorModel {
        calls: AtomicU32::new(0),
        first_call_gate: Arc::clone(&first_call_gate),
    });
    let config = AgentConfig::builder("research-pipeline/ordering")
        .system_prompt("Emit deterministic supervisor ordering milestones.")
        .max_steps(4)
        .build()
        .expect("build ordering supervisor");
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(NoopTool::new(PROBE_TOOL)))
        .expect("register activation probe");
    registry
        .register(Arc::new(NoopTool::new(ORDERED_TOOL)))
        .expect("register ordered work");
    let (handle, mut receiver) =
        AgentRun::start(config, "record watcher ordering".into(), model, registry);
    first_call_gate.wait_until_entered().await;

    let first_sequence = Arc::new(Mutex::new(Vec::new()));
    let first_activation = Arc::new(Notify::new());
    let first_terminal = Arc::new(Notify::new());
    handle
        .attach_watcher(
            Arc::new(CountingWatcher::until_terminal(
                Arc::clone(&first_sequence),
                Some((ACTIVATION_STEP, Arc::clone(&first_activation))),
                Arc::clone(&first_terminal),
            )),
            WATCHER_CAPACITY,
        )
        .await;

    let second_sequence = Arc::new(Mutex::new(Vec::new()));
    let second_activation = Arc::new(Notify::new());
    let second_terminal = Arc::new(Notify::new());
    handle
        .attach_watcher(
            Arc::new(CountingWatcher::until_terminal(
                Arc::clone(&second_sequence),
                Some((ACTIVATION_STEP, Arc::clone(&second_activation))),
                Arc::clone(&second_terminal),
            )),
            WATCHER_CAPACITY,
        )
        .await;

    first_call_gate.release();
    first_activation.notified().await;
    second_activation.notified().await;

    let mut primary_events = Vec::new();
    while let Some(event) = receiver.recv().await {
        let terminal = is_terminal(&event);
        primary_events.push(event);
        if terminal {
            break;
        }
    }
    first_terminal.notified().await;
    second_terminal.notified().await;
    handle.wait().await;

    assert!(primary_events.last().is_some_and(is_terminal));
    assert!(!primary_events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::EventsDropped { .. })));

    let first = first_sequence.lock().expect("first watcher sequence");
    let second = second_sequence.lock().expect("second watcher sequence");
    let milestones = [
        "model.started:1",
        "model.completed",
        "tool.started:ordered_work",
        "tool.completed:ordered_work",
        "model.started:2",
        "model.completed",
        "run.completed:EndTurn",
    ];
    assert_subsequence(&first, &milestones);
    assert_subsequence(&second, &milestones);

    let first_indexed = first.iter().cloned().enumerate().collect::<Vec<_>>();
    let second_indexed = second.iter().cloned().enumerate().collect::<Vec<_>>();
    assert_eq!(first_indexed, second_indexed);
    assert_eq!(
        first.last().map(String::as_str),
        Some("run.completed:EndTurn")
    );
    assert_eq!(
        second.last().map(String::as_str),
        Some("run.completed:EndTurn")
    );

    // This compares accepted event delivery only. CountingWatcher always
    // returns Continue, so equality makes no claim about cross-watcher action
    // application order.
    assert_eq!(
        stable_event_key(primary_events.last().expect("terminal event")),
        "run.completed:EndTurn"
    );
}
