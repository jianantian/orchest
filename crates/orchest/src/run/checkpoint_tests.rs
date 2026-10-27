//! Session checkpoint and resume-boundary tests (v2.0 issue 002).

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::*;
use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
};
use crate::session::{
    CheckpointPolicy, InMemorySessionStore, SessionEvent, SessionEventKind, SessionLog,
    SessionMeta, SessionStore,
};
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    Approval, JsonSchema, Tool, ToolCall, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput,
};

/// What the scripted model does on one call.
#[derive(Clone)]
enum Turn {
    CallTool {
        id: &'static str,
        name: &'static str,
    },
    Answer,
    Crash,
}

/// A model that plays `turns` in order and records the messages of every
/// call.
struct ScriptedModel {
    turns: Vec<Turn>,
    calls: AtomicUsize,
    seen: Mutex<Vec<Vec<Message>>>,
}

impl ScriptedModel {
    fn new(turns: Vec<Turn>) -> Arc<Self> {
        Arc::new(Self {
            turns,
            calls: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn seen(&self) -> Vec<Vec<Message>> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ModelAdapter for ScriptedModel {
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
        self.seen.lock().unwrap().push(messages.to_vec());
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        match self.turns.get(index).cloned().unwrap_or(Turn::Answer) {
            Turn::Crash => panic!("simulated crash"),
            Turn::Answer => Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
            Turn::CallTool { id, name } => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: id.into(),
                    name: name.into(),
                    input: json!({"path": "a.txt"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
        }
    }
}

/// A tool that records, each time it runs, whether its own dispatch marker
/// was already readable from the store.
struct ProbeTool {
    name: &'static str,
    metadata: ToolMetadata,
    store: Arc<InMemorySessionStore>,
    session_id: &'static str,
    runs: AtomicU32,
    marker_durable: Mutex<Vec<bool>>,
}

impl ProbeTool {
    fn new(
        name: &'static str,
        side_effect: bool,
        store: &Arc<InMemorySessionStore>,
        session_id: &'static str,
    ) -> Arc<Self> {
        Arc::new(Self {
            name,
            metadata: ToolMetadata {
                side_effect,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
            store: Arc::clone(store),
            session_id,
            runs: AtomicU32::new(0),
            marker_durable: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl Tool for ProbeTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "probe"
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
        self.runs.fetch_add(1, Ordering::SeqCst);
        let stored = self.store.read(self.session_id, 1).await.unwrap();
        let durable = stored.iter().any(|e| {
            matches!(&e.kind, SessionEventKind::ToolCall { call } if call.id == ctx.tool_call_id)
        });
        self.marker_durable.lock().unwrap().push(durable);
        Ok(ToolOutput::Immediate(json!("ok")))
    }
}

fn config(store: &Arc<InMemorySessionStore>, session_id: &str) -> AgentConfig {
    let mut config = AgentConfig::builder("checkpoint-agent", "mock")
        .system_prompt("system")
        .max_steps(10)
        .build()
        .unwrap()
        .with_session_store(Arc::clone(store) as Arc<dyn SessionStore>, session_id);
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

async fn stored(store: &InMemorySessionStore, session_id: &str) -> Vec<SessionEvent> {
    store.read(session_id, 1).await.unwrap()
}

fn tool_result_code(messages: &[Message], tool_use_id: &str) -> Option<String> {
    messages
        .iter()
        .flat_map(|m| &m.content)
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id: id,
                content,
            } if id == tool_use_id => Some(content["error"]["code"].as_str()?.to_string()),
            _ => None,
        })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn side_effect_marker_is_durable_before_dispatch() {
    let store = Arc::new(InMemorySessionStore::new());
    let write = ProbeTool::new("write", true, &store, "marker");
    let read = ProbeTool::new("read", false, &store, "marker");
    let mut registry = ToolRegistry::new();
    registry.register(write.clone()).unwrap();
    registry.register(read.clone()).unwrap();
    let model = ScriptedModel::new(vec![
        Turn::CallTool {
            id: "w1",
            name: "write",
        },
        Turn::CallTool {
            id: "r1",
            name: "read",
        },
        Turn::Answer,
    ]);

    let (handle, rx) = AgentRun::start(config(&store, "marker"), "go".into(), model, registry);
    drain(handle, rx).await;

    assert_eq!(*write.marker_durable.lock().unwrap(), vec![true]);
    assert_eq!(
        *read.marker_durable.lock().unwrap(),
        vec![false],
        "every_step does not flush before a read-only dispatch"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_tool_flushes_before_every_dispatch_and_records_the_policy() {
    let store = Arc::new(InMemorySessionStore::new());
    let read = ProbeTool::new("read", false, &store, "every-tool");
    let mut registry = ToolRegistry::new();
    registry.register(read.clone()).unwrap();
    let model = ScriptedModel::new(vec![
        Turn::CallTool {
            id: "r1",
            name: "read",
        },
        Turn::Answer,
    ]);
    let mut cfg = config(&store, "every-tool");
    cfg.checkpoint_policy = CheckpointPolicy::EveryTool;

    let (handle, rx) = AgentRun::start(cfg, "go".into(), model, registry);
    drain(handle, rx).await;

    assert_eq!(*read.marker_durable.lock().unwrap(), vec![true]);
    let meta = store.get_meta("every-tool").await.unwrap().unwrap();
    assert_eq!(meta.checkpoint_policy, CheckpointPolicy::EveryTool);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_mid_step_resumes_at_the_last_step_boundary() {
    let store = Arc::new(InMemorySessionStore::new());
    let write = ProbeTool::new("write", true, &store, "crash-step");
    let mut registry = ToolRegistry::new();
    registry.register(write.clone()).unwrap();
    // Step 0 runs the tool; step 1 crashes inside the model call.
    let model = ScriptedModel::new(vec![
        Turn::CallTool {
            id: "w1",
            name: "write",
        },
        Turn::Crash,
    ]);
    let (handle, rx) = AgentRun::start(
        config(&store, "crash-step"),
        "go".into(),
        model,
        registry.clone(),
    );
    drain(handle, rx).await;

    let before = SessionLog::from_events(stored(&store, "crash-step").await).unwrap();
    assert_eq!(before.projection().open_step, Some(1));
    let boundary_messages = before.messages().to_vec();
    let boundary_budget = before.projection().budget_used.clone();

    let model2 = ScriptedModel::new(vec![Turn::Answer]);
    let (handle2, rx2) = AgentRun::resume(config(&store, "crash-step"), model2.clone(), registry)
        .await
        .unwrap();
    drain(handle2, rx2).await;

    assert_eq!(model2.seen()[0], boundary_messages);
    assert_eq!(write.runs.load(Ordering::SeqCst), 1, "no tool runs again");
    let after = SessionLog::from_events(stored(&store, "crash-step").await).unwrap();
    let kinds: Vec<&SessionEventKind> = after.events()[before.events().len()..]
        .iter()
        .map(|e| &e.kind)
        .collect();
    assert!(matches!(
        kinds[0],
        SessionEventKind::StepInterrupted { step: 1 }
    ));
    assert!(matches!(
        kinds[2],
        SessionEventKind::StepStarted { step: 1 }
    ));
    assert!(after.projection().budget_used.tokens_used > boundary_budget.tokens_used);
}

/// Seeds a session whose open step requested `call` and, if `dispatched`,
/// recorded its dispatch marker before the crash.
async fn seed_open_step(store: &InMemorySessionStore, session_id: &str, dispatched: bool) {
    let call = ToolCall {
        id: "w1".into(),
        name: "write".into(),
        input: json!({"path": "a.txt"}),
    };
    let mut log = SessionLog::new();
    log.append(SessionEventKind::RunStarted {
        run_id: RunId::new(),
        agent_name: "checkpoint-agent".into(),
    });
    log.append(SessionEventKind::UserMessage {
        content: vec![ContentBlock::Text("go".into())],
    });
    log.append(SessionEventKind::StepStarted { step: 0 });
    log.append(SessionEventKind::AssistantMessage {
        content: vec![ContentBlock::ToolUse {
            id: call.id.clone(),
            name: call.name.clone(),
            input: call.input.clone(),
        }],
    });
    if dispatched {
        log.append(SessionEventKind::ToolCall { call });
    }
    store
        .put_meta(&SessionMeta::new(session_id, "checkpoint-agent"))
        .await
        .unwrap();
    store.append(session_id, log.events()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_before_dispatch_closes_the_call_as_not_started() {
    let store = Arc::new(InMemorySessionStore::new());
    seed_open_step(&store, "not-started", false).await;
    let write = ProbeTool::new("write", true, &store, "not-started");
    let mut registry = ToolRegistry::new();
    registry.register(write.clone()).unwrap();

    let model = ScriptedModel::new(vec![Turn::Answer]);
    let (handle, rx) = AgentRun::resume(config(&store, "not-started"), model.clone(), registry)
        .await
        .unwrap();
    drain(handle, rx).await;

    assert_eq!(write.runs.load(Ordering::SeqCst), 0);
    let first = &model.seen()[0];
    assert_eq!(
        tool_result_code(first, "w1").as_deref(),
        Some("TOOL_NOT_STARTED")
    );
    assert_eq!(first.last().map(|m| m.role), Some(Role::User));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_after_dispatch_closes_the_call_as_outcome_unknown() {
    let store = Arc::new(InMemorySessionStore::new());
    seed_open_step(&store, "unknown", true).await;
    let write = ProbeTool::new("write", true, &store, "unknown");
    let mut registry = ToolRegistry::new();
    registry.register(write.clone()).unwrap();

    let model = ScriptedModel::new(vec![Turn::Answer]);
    let (handle, rx) = AgentRun::resume(config(&store, "unknown"), model.clone(), registry)
        .await
        .unwrap();
    drain(handle, rx).await;

    assert_eq!(write.runs.load(Ordering::SeqCst), 0, "never re-run");
    assert_eq!(
        tool_result_code(&model.seen()[0], "w1").as_deref(),
        Some("TOOL_OUTCOME_UNKNOWN")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_after_run_end_projects_the_same_history() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = ScriptedModel::new(vec![Turn::Answer]);
    let (handle, rx) = AgentRun::start(
        config(&store, "ended"),
        "go".into(),
        model,
        ToolRegistry::new(),
    );
    drain(handle, rx).await;
    let before = SessionLog::from_events(stored(&store, "ended").await).unwrap();

    let model2 = ScriptedModel::new(vec![Turn::Answer]);
    let (handle2, rx2) =
        AgentRun::resume(config(&store, "ended"), model2.clone(), ToolRegistry::new())
            .await
            .unwrap();
    drain(handle2, rx2).await;

    assert_eq!(model2.seen()[0], before.messages());
    let after = SessionLog::from_events(stored(&store, "ended").await).unwrap();
    assert!(
        !after.events()[before.events().len()..]
            .iter()
            .any(|e| matches!(e.kind, SessionEventKind::StepInterrupted { .. })),
        "a closed log needs no repair"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_records_a_terminal_event() {
    struct BlockingModel;
    #[async_trait::async_trait]
    impl ModelAdapter for BlockingModel {
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
            _tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: format!("t{}", uuid::Uuid::new_v4()),
                    name: "missing".into(),
                    input: json!({}),
                }],
                usage: TokenUsage::default(),
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }

    let store = Arc::new(InMemorySessionStore::new());
    let mut cfg = config(&store, "aborted");
    cfg.runtime.max_steps = 1000;
    let (handle, mut rx) = AgentRun::start(
        cfg,
        "go".into(),
        Arc::new(BlockingModel),
        ToolRegistry::new(),
    );
    // Wait until the run is underway, then abort it.
    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ModelCallCompleted { .. }) {
            break;
        }
    }
    handle.abort();
    while rx.recv().await.is_some() {}
    handle.wait().await;

    let log = SessionLog::from_events(stored(&store, "aborted").await).unwrap();
    assert!(matches!(
        log.events().last().map(|e| &e.kind),
        Some(SessionEventKind::RunAborted { .. })
    ));
}
