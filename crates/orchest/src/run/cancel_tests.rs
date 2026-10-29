//! Cancellation tests (v2.0 issue 006).

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::{mpsc, Notify};

use super::checkpoint_tests::tool_result_code;
use super::*;
use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use crate::session::{InMemorySessionStore, SessionLog, SessionStore};
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolEffect, ToolEffects, ToolError,
    ToolMetadata, ToolOutput, TOOL_ABORTED_BEFORE_DISPATCH, TOOL_OUTCOME_UNKNOWN,
};

/// What the model does on each call.
#[derive(Clone)]
enum Reply {
    /// Never returns; signals `entered` first.
    Hang,
    /// Requests these tools in one response.
    Tools(Vec<(&'static str, &'static str)>),
    Answer,
}

struct Model {
    replies: Vec<Reply>,
    calls: AtomicUsize,
    entered: Arc<Notify>,
    seen: Mutex<Vec<Vec<Message>>>,
}

impl Model {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies,
            calls: AtomicUsize::new(0),
            entered: Arc::new(Notify::new()),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ModelAdapter for Model {
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
        let response = |content, stop_reason| ModelResponse {
            content,
            usage: TokenUsage::default(),
            stop_reason,
            option_adjustments: vec![],
        };
        match self.replies.get(index).cloned().unwrap_or(Reply::Answer) {
            Reply::Hang => {
                self.entered.notify_one();
                std::future::pending().await
            }
            Reply::Tools(calls) => Ok(response(
                calls
                    .into_iter()
                    .map(|(id, name)| ContentBlock::ToolUse {
                        id: id.into(),
                        name: name.into(),
                        input: json!({}),
                    })
                    .collect(),
                StopReason::ToolUse,
            )),
            Reply::Answer => Ok(response(
                vec![ContentBlock::Text("done".into())],
                StopReason::EndTurn,
            )),
        }
    }
}

/// A tool that signals `entered` and then waits for `release` (or forever).
struct GateTool {
    name: &'static str,
    metadata: ToolMetadata,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    runs: AtomicU32,
    finished: AtomicU32,
}

impl GateTool {
    fn new(name: &'static str, effects: ToolEffects) -> Arc<Self> {
        Arc::new(Self {
            name,
            metadata: ToolMetadata {
                effects,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            runs: AtomicU32::new(0),
            finished: AtomicU32::new(0),
        })
    }
}

#[async_trait::async_trait]
impl Tool for GateTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "gate"
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
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
        self.finished.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::Immediate(json!("released")))
    }
}

fn config(store: &Arc<InMemorySessionStore>, session_id: &str) -> AgentConfig {
    let mut config = AgentConfig::builder("cancel-agent", "mock")
        .system_prompt("system")
        .max_steps(10)
        .build()
        .unwrap()
        .with_session_store(Arc::clone(store) as Arc<dyn SessionStore>, session_id);
    config.supervision_strategy = SupervisionStrategy::Stop;
    config
}

fn registry(tools: &[Arc<GateTool>]) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    for tool in tools {
        registry
            .register(Arc::clone(tool) as Arc<dyn Tool>)
            .unwrap();
    }
    registry
}

/// Collects every event of the run, failing the test if it does not end
/// within a few seconds.
async fn collect(mut rx: EventReceiver) -> Vec<RuntimeEvent> {
    tokio::time::timeout(Duration::from_secs(5), async move {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    })
    .await
    .expect("run did not end after cancellation")
}

fn aborts(events: &[RuntimeEvent]) -> Vec<Option<CancelReason>> {
    events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::RunAborted { cancel_reason, .. } => Some(*cancel_reason),
            _ => None,
        })
        .collect()
}

fn terminals(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|e| {
            matches!(
                e,
                RuntimeEvent::RunCompleted { .. }
                    | RuntimeEvent::RunFailed { .. }
                    | RuntimeEvent::RunAborted { .. }
            )
        })
        .count()
}

async fn stored_log(store: &InMemorySessionStore, session_id: &str) -> SessionLog {
    SessionLog::from_events(store.read(session_id, 1).await.unwrap()).unwrap()
}

#[tokio::test]
async fn cancel_interrupts_the_model_call() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Model::new(vec![Reply::Hang]);
    let (handle, rx) = AgentRun::start(
        config(&store, "model"),
        "go".into(),
        model.clone(),
        registry(&[]),
    );
    let events = tokio::spawn(collect(rx));
    model.entered.notified().await;

    assert!(handle.cancel(Cancellation::new(CancelReason::Timeout).with_message("deadline")));
    let events = events.await.unwrap();
    handle.wait().await;

    assert_eq!(aborts(&events), vec![Some(CancelReason::Timeout)]);
    assert_eq!(terminals(&events), 1);
    assert!(events.iter().any(|e| matches!(
        e,
        RuntimeEvent::RunAborted { reason: Some(r), .. } if r == "deadline"
    )));
    assert_eq!(model.calls(), 1);
}

#[tokio::test]
async fn cancel_drops_an_interruptible_tool_and_closes_it_in_the_ledger() {
    let store = Arc::new(InMemorySessionStore::new());
    let read = GateTool::new("read", ToolEffects::READ_ONLY);
    let model = Model::new(vec![Reply::Tools(vec![("r1", "read")])]);
    let (handle, rx) = AgentRun::start(
        config(&store, "interruptible"),
        "go".into(),
        model.clone(),
        registry(std::slice::from_ref(&read)),
    );
    let events = tokio::spawn(collect(rx));
    read.entered.notified().await;

    handle.abort();
    let events = events.await.unwrap();
    handle.wait().await;

    assert_eq!(aborts(&events), vec![Some(CancelReason::User)]);
    assert_eq!(read.finished.load(Ordering::SeqCst), 0);
    assert_eq!(model.calls(), 1);
    let log = stored_log(&store, "interruptible").await;
    assert_eq!(
        tool_result_code(log.messages(), "r1").as_deref(),
        Some(TOOL_OUTCOME_UNKNOWN)
    );
}

#[tokio::test]
async fn non_interruptible_tool_finishes_and_nothing_new_starts() {
    let store = Arc::new(InMemorySessionStore::new());
    let write = GateTool::new("write", ToolEffects::NONE.with(ToolEffect::WriteWorkspace));
    let second = GateTool::new("second", ToolEffects::NONE.with(ToolEffect::WriteWorkspace));
    let model = Model::new(vec![Reply::Tools(vec![("w1", "write"), ("w2", "second")])]);
    let (handle, rx) = AgentRun::start(
        config(&store, "finish"),
        "go".into(),
        model.clone(),
        registry(&[write.clone(), second.clone()]),
    );
    let events = tokio::spawn(collect(rx));
    write.entered.notified().await;

    handle.abort();
    tokio::task::yield_now().await;
    write.release.notify_one();
    let events = events.await.unwrap();
    handle.wait().await;

    assert_eq!(write.finished.load(Ordering::SeqCst), 1);
    assert_eq!(second.runs.load(Ordering::SeqCst), 0);
    assert_eq!(model.calls(), 1);
    assert_eq!(aborts(&events), vec![Some(CancelReason::User)]);
    let log = stored_log(&store, "finish").await;
    let messages = log.messages();
    assert_eq!(tool_result_code(messages, "w1"), None, "w1 result is real");
    assert!(messages.iter().flat_map(|m| &m.content).any(|b| matches!(
        b,
        ContentBlock::ToolResult { tool_use_id, content }
            if tool_use_id == "w1" && content == &json!("released")
    )));
    assert_eq!(
        tool_result_code(messages, "w2").as_deref(),
        Some(TOOL_ABORTED_BEFORE_DISPATCH)
    );
}

#[tokio::test]
async fn cancel_resolves_a_pending_approval_at_once() {
    let store = Arc::new(InMemorySessionStore::new());
    let mut guarded = GateTool::new("guarded", ToolEffects::UNKNOWN);
    Arc::get_mut(&mut guarded).unwrap().metadata.approval = Approval::Always;
    let model = Model::new(vec![Reply::Tools(vec![("g1", "guarded")])]);
    let mut config = config(&store, "approval");
    config.runtime.request_timeout = Duration::from_secs(3600);
    let (handle, mut rx) = AgentRun::start(
        config,
        "go".into(),
        model.clone(),
        registry(&[guarded.clone()]),
    );

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ApprovalRequested { .. }) {
            handle.abort();
        }
        events.push(event);
    }
    assert!(handle.pending_requests().await.is_empty());
    handle.wait().await;

    assert!(events.iter().any(|e| matches!(
        e,
        RuntimeEvent::RequestResolved {
            resolution: RequestResolution::Cancelled,
            ..
        }
    )));
    assert_eq!(guarded.runs.load(Ordering::SeqCst), 0);
    assert_eq!(aborts(&events), vec![Some(CancelReason::User)]);
    assert_eq!(model.calls(), 1);
}

#[tokio::test]
async fn cancel_is_idempotent() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Model::new(vec![Reply::Hang]);
    let (handle, rx) = AgentRun::start(
        config(&store, "twice"),
        "go".into(),
        model.clone(),
        registry(&[]),
    );
    let events = tokio::spawn(collect(rx));
    model.entered.notified().await;

    assert!(handle.cancel(Cancellation::new(CancelReason::HostDisconnected)));
    assert!(!handle.cancel(Cancellation::user()));
    handle.abort();
    let events = events.await.unwrap();
    assert_eq!(
        handle.cancellation().unwrap().reason,
        CancelReason::HostDisconnected
    );
    handle.wait().await;

    assert_eq!(aborts(&events), vec![Some(CancelReason::HostDisconnected)]);
    assert_eq!(terminals(&events), 1);
}

#[tokio::test]
async fn cancel_after_completion_adds_no_terminal() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Model::new(vec![Reply::Answer]);
    let (handle, rx) = AgentRun::start(
        config(&store, "late"),
        "go".into(),
        model.clone(),
        registry(&[]),
    );
    let mut rx = rx;
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let completed = matches!(event, RuntimeEvent::RunCompleted { .. });
        events.push(event);
        if completed {
            handle.abort();
        }
    }
    handle.wait().await;

    assert_eq!(terminals(&events), 1);
    assert!(aborts(&events).is_empty());
}

struct Terminator {
    calls: Mutex<Vec<(RunId, CancelReason)>>,
    outcome: Result<usize, String>,
}

#[async_trait::async_trait]
impl ProcessTerminator for Terminator {
    async fn terminate(&self, run_id: RunId, cancellation: &Cancellation) -> Result<usize, String> {
        self.calls
            .lock()
            .unwrap()
            .push((run_id, cancellation.reason));
        self.outcome.clone()
    }
}

#[tokio::test]
async fn process_terminator_runs_on_cancel_and_failures_are_reported() {
    for outcome in [Ok(2), Err("permission denied".to_string())] {
        let store = Arc::new(InMemorySessionStore::new());
        let terminator = Arc::new(Terminator {
            calls: Mutex::new(Vec::new()),
            outcome: outcome.clone(),
        });
        let mut config = config(&store, "terminate");
        config.runtime.process_terminator = Some(terminator.clone());
        let model = Model::new(vec![Reply::Hang]);
        let (handle, rx) = AgentRun::start(config, "go".into(), model.clone(), registry(&[]));
        let run_id = handle.run_id;
        let events = tokio::spawn(collect(rx));
        model.entered.notified().await;

        handle.abort();
        handle.abort();
        let events = events.await.unwrap();
        handle.wait().await;

        assert_eq!(
            *terminator.calls.lock().unwrap(),
            vec![(run_id, CancelReason::User)]
        );
        let reported = events.iter().find_map(|e| match e {
            RuntimeEvent::ProcessesTerminated { terminated, error } => {
                Some((*terminated, error.clone()))
            }
            _ => None,
        });
        match outcome {
            Ok(count) => assert_eq!(reported, Some((Some(count), None))),
            Err(error) => assert_eq!(reported, Some((None, Some(error)))),
        }
    }
}

#[tokio::test]
async fn host_can_make_a_read_only_tool_run_to_completion() {
    let store = Arc::new(InMemorySessionStore::new());
    let read = GateTool::new("read", ToolEffects::READ_ONLY);
    let model = Model::new(vec![Reply::Tools(vec![("r1", "read")])]);
    let mut config = config(&store, "override");
    config.runtime.interruptible.insert("read".into(), false);
    let (handle, rx) = AgentRun::start(
        config,
        "go".into(),
        model.clone(),
        registry(std::slice::from_ref(&read)),
    );
    let events = tokio::spawn(collect(rx));
    read.entered.notified().await;

    handle.abort();
    tokio::task::yield_now().await;
    read.release.notify_one();
    events.await.unwrap();
    handle.wait().await;

    assert_eq!(read.finished.load(Ordering::SeqCst), 1);
}
