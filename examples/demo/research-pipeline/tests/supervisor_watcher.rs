use std::{
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use orchest::{
    events::RuntimeEvent,
    model::{
        ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
        RequestOptions, Role, StopReason, StreamEvent, TokenUsage, ToolDef,
    },
    run::{llm_watcher::LlmWatcher, AgentRun, RunHandle, RunId, WatcherAction},
    tool::{
        agent_as_tool::ContextMode, Approval, Tool, ToolContext, ToolError, ToolMetadata,
        ToolOutput, ToolSource,
    },
};
use research_pipeline_demo::{
    supervisor::{
        await_delegated_worker_completion, build_supervisor, resolve_delegated_worker_target,
        start_with_live_watchers, LIVE_ATTACHMENT_BOUNDARY, RESEARCH_WORKER_TOOL,
    },
    watcher::{RecordingActionWatcher, RecordingLlmWatcher},
    worker::Worker,
};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Notify};

const PROBE_TOOL: &str = "supervisor_probe";
const CHECKPOINT_TOOL: &str = "supervisor_checkpoint";
const WATCHER_INJECT: &str = "watcher injects into supervisor";
const WATCHER_STEER: &str = "watcher steers supervisor";
const EXTERNAL_INJECT: &str = "external injects into supervisor";
const EXTERNAL_STEER: &str = "external steers supervisor";
const ACTIVATION_STEP: u32 = 1;

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

struct TwoStageSupervisorModel {
    calls: AtomicU32,
    first_call_gate: Arc<CallGate>,
    delegation_call_gate: Arc<CallGate>,
    histories: Arc<Mutex<Vec<Vec<Message>>>>,
    required_messages: Vec<(Role, &'static str)>,
}

#[async_trait]
impl ModelAdapter for TwoStageSupervisorModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "two-stage-supervisor"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        if tools.iter().any(|tool| tool.name == "decide_action") {
            return Ok(action_response("continue", ""));
        }

        self.histories
            .lock()
            .expect("supervisor history lock")
            .push(messages.to_vec());
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        match call {
            0 => {
                self.first_call_gate.hold().await;
                Ok(tool_response("probe-1", PROBE_TOOL))
            }
            1 => {
                self.delegation_call_gate.hold().await;
                Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "delegate-1".to_string(),
                        name: RESEARCH_WORKER_TOOL.to_string(),
                        input: json!({"input": "delegated child request"}),
                    }],
                    usage: TokenUsage::default(),
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                })
            }
            _ if self
                .required_messages
                .iter()
                .all(|(role, text)| conversation_contains(messages, *role, text)) =>
            {
                Ok(ModelResponse {
                    content: vec![ContentBlock::Text("supervisor complete".to_string())],
                    usage: TokenUsage::default(),
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                })
            }
            _ => Ok(tool_response(
                &format!("checkpoint-{call}"),
                CHECKPOINT_TOOL,
            )),
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

fn action_response(action: &str, message: &str) -> ModelResponse {
    ModelResponse {
        content: vec![ContentBlock::ToolUse {
            id: "watcher-decision".to_string(),
            name: "decide_action".to_string(),
            input: json!({"action": action, "message": message}),
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
        "Yield once and return a deterministic supervisor checkpoint."
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
        tokio::task::yield_now().await;
        Ok(ToolOutput::Immediate(json!({"checkpoint": self.name})))
    }
}

struct CapturingChildModel {
    histories: Arc<Mutex<Vec<Vec<Message>>>>,
}

#[async_trait]
impl ModelAdapter for CapturingChildModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "capturing-child"
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
        self.histories
            .lock()
            .expect("child history lock")
            .push(messages.to_vec());
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("worker complete".to_string())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

struct WatcherDecisionModel {
    prompts: Arc<Mutex<Vec<String>>>,
    terminal_prompt_recorded: Arc<Notify>,
}

#[async_trait]
impl ModelAdapter for WatcherDecisionModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "watcher-decision"
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
        let prompt = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.prompts
            .lock()
            .expect("watcher prompt lock")
            .push(prompt.clone());
        if prompt.contains("Run completed") {
            self.terminal_prompt_recorded.notify_one();
        }

        if prompt.contains(&format!("Tool called: {RESEARCH_WORKER_TOOL}")) {
            Ok(action_response("steer", WATCHER_STEER))
        } else {
            Ok(action_response("continue", ""))
        }
    }
}

struct DeterministicWatchers {
    recording_events: Arc<Mutex<Vec<RuntimeEvent>>>,
    recording_activation_processed: Arc<Notify>,
    recording_terminal_processed: Arc<Notify>,
    llm_completed_events: Arc<Mutex<Vec<RuntimeEvent>>>,
    llm_activation_processed: Arc<Notify>,
    llm_terminal_processed: Arc<Notify>,
    llm_prompts: Arc<Mutex<Vec<String>>>,
    llm_terminal_prompt_recorded: Arc<Notify>,
}

async fn attach_deterministic_watchers(handle: &RunHandle) -> DeterministicWatchers {
    let recording_events = Arc::new(Mutex::new(Vec::new()));
    let recording_activation_processed = Arc::new(Notify::new());
    let recording_terminal_processed = Arc::new(Notify::new());
    handle
        .attach_watcher(
            Arc::new(RecordingActionWatcher::on_tool_until_terminal(
                Arc::clone(&recording_events),
                RESEARCH_WORKER_TOOL,
                WatcherAction::Inject(WATCHER_INJECT.to_string()),
                (ACTIVATION_STEP, Arc::clone(&recording_activation_processed)),
                Arc::clone(&recording_terminal_processed),
            )),
            1024,
        )
        .await;

    let llm_prompts = Arc::new(Mutex::new(Vec::new()));
    let llm_terminal_prompt_recorded = Arc::new(Notify::new());
    let watcher_model: Arc<dyn ModelAdapter> = Arc::new(WatcherDecisionModel {
        prompts: Arc::clone(&llm_prompts),
        terminal_prompt_recorded: Arc::clone(&llm_terminal_prompt_recorded),
    });
    let llm_watcher = LlmWatcher::builder()
        .eval_interval(1)
        .model(Arc::clone(&watcher_model))
        .build()
        .expect("watcher model is configured");
    let llm_completed_events = Arc::new(Mutex::new(Vec::new()));
    let llm_activation_processed = Arc::new(Notify::new());
    let llm_terminal_processed = Arc::new(Notify::new());
    handle
        .attach_watcher(
            Arc::new(RecordingLlmWatcher::until_terminal(
                llm_watcher,
                Arc::clone(&llm_completed_events),
                Some((ACTIVATION_STEP, Arc::clone(&llm_activation_processed))),
                Arc::clone(&llm_terminal_processed),
            )),
            1024,
        )
        .await;

    DeterministicWatchers {
        recording_events,
        recording_activation_processed,
        recording_terminal_processed,
        llm_completed_events,
        llm_activation_processed,
        llm_terminal_processed,
        llm_prompts,
        llm_terminal_prompt_recorded,
    }
}

fn worker_fixture() -> (tempfile::TempDir, Worker) {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let corpus = temp.path().join("corpus");
    std::fs::create_dir(&corpus).expect("create corpus");
    std::fs::write(
        corpus.join("source.md"),
        "# Source\nDeterministic delegation evidence.",
    )
    .expect("write source fixture");
    let worker = Worker::from_paths(&corpus, temp.path().join("draft.md")).expect("build worker");
    (temp, worker)
}

fn message_contains(message: &Message, expected: &str) -> bool {
    message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text(text) if text == expected))
}

fn conversation_contains(messages: &[Message], role: Role, expected: &str) -> bool {
    messages
        .iter()
        .any(|message| message.role == role && message_contains(message, expected))
}

fn has_activation_event(events: &[RuntimeEvent]) -> bool {
    events.iter().any(
        |event| matches!(event, RuntimeEvent::ModelCallStarted { step } if *step == ACTIVATION_STEP),
    )
}

fn child_run_id(events: &[RuntimeEvent]) -> RunId {
    events
        .iter()
        .find_map(|event| match event {
            RuntimeEvent::SubAgentStarted { child_run_id, .. } => Some(*child_run_id),
            _ => None,
        })
        .expect("delegation emits SubAgentStarted")
}

fn assert_completed(events: &[RuntimeEvent]) {
    assert!(events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunCompleted { .. })));
    assert!(!events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunFailed { .. })));
}

async fn collect_to_terminal(
    handle: RunHandle,
    mut receiver: orchest::run::EventReceiver,
) -> Vec<RuntimeEvent> {
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    handle.wait().await;
    assert_completed(&events);
    events
}

struct Scenario {
    config: orchest::run::AgentConfig,
    registry: orchest::tool::registry::ToolRegistry,
    supervisor_model: Arc<dyn ModelAdapter>,
    first_call_gate: Arc<CallGate>,
    delegation_call_gate: Arc<CallGate>,
    supervisor_histories: Arc<Mutex<Vec<Vec<Message>>>>,
    child_histories: Arc<Mutex<Vec<Vec<Message>>>>,
}

fn build_scenario(
    worker: &Worker,
    context_mode: ContextMode,
    required_messages: Vec<(Role, &'static str)>,
) -> Scenario {
    let first_call_gate = Arc::new(CallGate::default());
    let delegation_call_gate = Arc::new(CallGate::default());
    let supervisor_histories = Arc::new(Mutex::new(Vec::new()));
    let child_histories = Arc::new(Mutex::new(Vec::new()));
    let supervisor_model: Arc<dyn ModelAdapter> = Arc::new(TwoStageSupervisorModel {
        calls: AtomicU32::new(0),
        first_call_gate: Arc::clone(&first_call_gate),
        delegation_call_gate: Arc::clone(&delegation_call_gate),
        histories: Arc::clone(&supervisor_histories),
        required_messages,
    });
    let child_model: Arc<dyn ModelAdapter> = Arc::new(CapturingChildModel {
        histories: Arc::clone(&child_histories),
    });
    let (mut config, mut registry) =
        build_supervisor(worker, child_model, context_mode, false).expect("build supervisor");
    config.runtime.max_steps = 8;
    registry
        .register(Arc::new(NoopTool::new(PROBE_TOOL)))
        .expect("register probe tool");
    registry
        .register(Arc::new(NoopTool::new(CHECKPOINT_TOOL)))
        .expect("register checkpoint tool");
    Scenario {
        config,
        registry,
        supervisor_model,
        first_call_gate,
        delegation_call_gate,
        supervisor_histories,
        child_histories,
    }
}

#[tokio::test]
async fn activated_watchers_prove_nested_routing_and_applied_supervisor_actions() {
    let (_temp, worker) = worker_fixture();
    let Scenario {
        config,
        registry,
        supervisor_model,
        first_call_gate,
        delegation_call_gate,
        supervisor_histories,
        child_histories,
    } = build_scenario(
        &worker,
        ContextMode::Fresh,
        vec![
            (Role::User, WATCHER_INJECT),
            (Role::System, WATCHER_STEER),
            (Role::User, EXTERNAL_INJECT),
            (Role::System, EXTERNAL_STEER),
        ],
    );
    let (handle, mut receiver) = AgentRun::start(
        config,
        "parent research question".into(),
        supervisor_model,
        registry,
    );
    first_call_gate.wait_until_entered().await;
    let watchers = attach_deterministic_watchers(&handle).await;

    first_call_gate.release();
    delegation_call_gate.wait_until_entered().await;
    watchers.recording_activation_processed.notified().await;
    watchers.llm_activation_processed.notified().await;
    assert!(has_activation_event(
        &watchers.recording_events.lock().expect("recording events")
    ));
    assert!(has_activation_event(
        &watchers
            .llm_completed_events
            .lock()
            .expect("LLM completed events")
    ));

    handle.inject_message(EXTERNAL_INJECT);
    handle.steer(EXTERNAL_STEER);
    delegation_call_gate.release();

    let mut primary_events = Vec::new();
    while let Some(event) = receiver.recv().await {
        primary_events.push(event);
    }
    watchers.recording_terminal_processed.notified().await;
    watchers.llm_terminal_processed.notified().await;
    watchers.llm_terminal_prompt_recorded.notified().await;
    assert_completed(&primary_events);

    let observed_child_run_id = child_run_id(&primary_events);
    assert!(primary_events.iter().any(|event| matches!(
        event,
        RuntimeEvent::SubAgentEvent { child_run_id, event, .. }
            if *child_run_id == observed_child_run_id
                && matches!(event.as_ref(), RuntimeEvent::RunCompleted { .. })
    )));
    let target = resolve_delegated_worker_target(&handle, observed_child_run_id)
        .await
        .expect("public child control surface resolves after SubAgentStarted");
    assert_ne!(target.owned_supervisor_run_id, target.observed_child_run_id);
    assert_eq!(target.child_parent_run_id, target.owned_supervisor_run_id);
    let child_outcome = await_delegated_worker_completion(&handle, observed_child_run_id)
        .await
        .expect("child completion is awaitable without supervisor EventReceiver");
    assert!(matches!(
        child_outcome,
        orchest::run::ChildRunOutcome::Completed { .. }
    ));
    handle.wait().await;

    let recording_events = watchers.recording_events.lock().expect("recording events");
    assert!(recording_events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunCompleted { .. })));
    assert!(
        recording_events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::SubAgentEvent { .. })),
        "attached recording watcher must receive forwarded SubAgentEvent"
    );
    drop(recording_events);

    let llm_completed_events = watchers
        .llm_completed_events
        .lock()
        .expect("LLM completed events");
    assert!(llm_completed_events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunCompleted { .. })));
    assert!(
        llm_completed_events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::SubAgentEvent { .. })),
        "attached LlmWatcher must receive forwarded SubAgentEvent"
    );
    drop(llm_completed_events);

    let llm_prompts = watchers.llm_prompts.lock().expect("LLM prompts");
    assert!(llm_prompts
        .iter()
        .any(|prompt| prompt.contains("Run completed")));
    assert!(
        llm_prompts.iter().any(|prompt| {
            prompt.contains("Sub-agent event")
                || prompt.contains("Sub-agent started")
                || prompt.contains("Sub-agent completed")
        }),
        "LlmWatcher prompts must use structured nested formatting, not Debug"
    );
    assert!(
        !llm_prompts
            .iter()
            .any(|prompt| prompt.contains("SubAgentEvent")),
        "LlmWatcher must not fall back to Debug SubAgentEvent text"
    );
    drop(llm_prompts);

    let supervisor_calls = supervisor_histories.lock().expect("supervisor histories");
    let final_supervisor_call = supervisor_calls.last().expect("final supervisor call");
    for (role, expected) in [
        (Role::User, WATCHER_INJECT),
        (Role::System, WATCHER_STEER),
        (Role::User, EXTERNAL_INJECT),
        (Role::System, EXTERNAL_STEER),
    ] {
        assert!(conversation_contains(final_supervisor_call, role, expected));
    }
    drop(supervisor_calls);

    let child_calls = child_histories.lock().expect("child histories");
    let first_child_call = child_calls.first().expect("first child call");
    assert_eq!(
        first_child_call.len(),
        2,
        "Fresh reuses the issue #245 no-parent-history assertion"
    );
    for steering_text in [
        WATCHER_INJECT,
        WATCHER_STEER,
        EXTERNAL_INJECT,
        EXTERNAL_STEER,
    ] {
        assert!(first_child_call
            .iter()
            .all(|message| !message_contains(message, steering_text)));
    }
    drop(child_calls);
}

#[derive(Clone, Copy)]
enum SteeringPath {
    WatcherInject,
    WatcherSteer,
    ExternalInject,
    ExternalSteer,
}

impl SteeringPath {
    fn expected(self) -> (Role, &'static str) {
        match self {
            Self::WatcherInject => (Role::User, WATCHER_INJECT),
            Self::WatcherSteer => (Role::System, WATCHER_STEER),
            Self::ExternalInject => (Role::User, EXTERNAL_INJECT),
            Self::ExternalSteer => (Role::System, EXTERNAL_STEER),
        }
    }

    fn watcher_action(self) -> Option<WatcherAction> {
        match self {
            Self::WatcherInject => Some(WatcherAction::Inject(WATCHER_INJECT.to_string())),
            Self::WatcherSteer => Some(WatcherAction::Steer(WATCHER_STEER.to_string())),
            Self::ExternalInject | Self::ExternalSteer => None,
        }
    }
}

async fn assert_single_steering_path_targets_supervisor(path: SteeringPath) {
    let (_temp, worker) = worker_fixture();
    let (role, expected) = path.expected();
    let Scenario {
        config,
        registry,
        supervisor_model,
        first_call_gate,
        delegation_call_gate,
        supervisor_histories,
        child_histories,
    } = build_scenario(&worker, ContextMode::Fresh, vec![(role, expected)]);
    let (handle, receiver) = AgentRun::start(
        config,
        "parent research question".into(),
        supervisor_model,
        registry,
    );
    first_call_gate.wait_until_entered().await;

    let activation_processed = Arc::new(Notify::new());
    let terminal_processed = Arc::new(Notify::new());
    if let Some(action) = path.watcher_action() {
        handle
            .attach_watcher(
                Arc::new(RecordingActionWatcher::on_tool_until_terminal(
                    Arc::new(Mutex::new(Vec::new())),
                    RESEARCH_WORKER_TOOL,
                    action,
                    (ACTIVATION_STEP, Arc::clone(&activation_processed)),
                    Arc::clone(&terminal_processed),
                )),
                1024,
            )
            .await;
    }

    first_call_gate.release();
    delegation_call_gate.wait_until_entered().await;
    if path.watcher_action().is_some() {
        activation_processed.notified().await;
    } else {
        match path {
            SteeringPath::ExternalInject => handle.inject_message(EXTERNAL_INJECT),
            SteeringPath::ExternalSteer => handle.steer(EXTERNAL_STEER),
            SteeringPath::WatcherInject | SteeringPath::WatcherSteer => unreachable!(),
        }
    }
    delegation_call_gate.release();
    let events = collect_to_terminal(handle, receiver).await;
    assert_completed(&events);
    if path.watcher_action().is_some() {
        terminal_processed.notified().await;
    }

    let supervisor_calls = supervisor_histories.lock().expect("supervisor histories");
    assert!(supervisor_calls
        .last()
        .is_some_and(|messages| conversation_contains(messages, role, expected)));
    drop(supervisor_calls);
    let child_calls = child_histories.lock().expect("child histories");
    assert!(child_calls
        .iter()
        .flatten()
        .all(|message| !message_contains(message, expected)));
}

#[tokio::test]
async fn watcher_inject_targets_the_supervisor_conversation() {
    assert_single_steering_path_targets_supervisor(SteeringPath::WatcherInject).await;
}

#[tokio::test]
async fn watcher_steer_targets_the_supervisor_conversation() {
    assert_single_steering_path_targets_supervisor(SteeringPath::WatcherSteer).await;
}

#[tokio::test]
async fn external_inject_targets_the_supervisor_conversation() {
    assert_single_steering_path_targets_supervisor(SteeringPath::ExternalInject).await;
}

#[tokio::test]
async fn external_steer_targets_the_supervisor_conversation() {
    assert_single_steering_path_targets_supervisor(SteeringPath::ExternalSteer).await;
}

#[tokio::test]
async fn fork_delegation_reuses_the_bounded_parent_tail() {
    let (_temp, worker) = worker_fixture();
    let depth = NonZeroUsize::new(2).expect("non-zero depth");
    let Scenario {
        config,
        registry,
        supervisor_model,
        first_call_gate,
        delegation_call_gate,
        supervisor_histories: _,
        child_histories,
    } = build_scenario(&worker, ContextMode::Fork { depth }, vec![]);
    let (handle, receiver) = AgentRun::start(
        config,
        "parent research question".into(),
        supervisor_model,
        registry,
    );
    first_call_gate.wait_until_entered().await;
    first_call_gate.release();
    delegation_call_gate.wait_until_entered().await;
    delegation_call_gate.release();
    let _events = collect_to_terminal(handle, receiver).await;

    let child_calls = child_histories.lock().expect("child histories");
    let first_child_call = child_calls.first().expect("first child call");
    assert_eq!(
        first_child_call.len(),
        4,
        "Fork depth two adds exactly two parent messages"
    );
    assert!(matches!(first_child_call[0].role, Role::System));
    assert!(matches!(first_child_call[1].role, Role::User));
    assert!(first_child_call[1].content.iter().any(|block| matches!(
        block,
        ContentBlock::ToolResult { content, .. }
            if content.get("checkpoint").and_then(Value::as_str) == Some(PROBE_TOOL)
    )));
    assert!(matches!(first_child_call[2].role, Role::Assistant));
    assert!(first_child_call[2].content.iter().any(|block| matches!(
        block,
        ContentBlock::ToolUse { name, .. } if name == RESEARCH_WORKER_TOOL
    )));
    assert!(
        !conversation_contains(first_child_call, Role::User, "parent research question"),
        "Fork depth two must exclude messages before the bounded parent tail"
    );
    assert!(conversation_contains(
        first_child_call,
        Role::User,
        "delegated child request"
    ));
}

#[tokio::test]
async fn live_start_records_both_watcher_completions_as_best_effort() {
    let (_temp, worker) = worker_fixture();
    let Scenario {
        config,
        registry,
        supervisor_model,
        first_call_gate,
        delegation_call_gate,
        supervisor_histories: _,
        child_histories: _,
    } = build_scenario(&worker, ContextMode::Fresh, vec![]);
    let started = start_with_live_watchers(
        config,
        "live-shaped parent question".to_string(),
        supervisor_model,
        registry,
    )
    .await
    .expect("live watchers attach with configured model");
    first_call_gate.wait_until_entered().await;
    first_call_gate.release();
    delegation_call_gate.wait_until_entered().await;
    delegation_call_gate.release();

    let mut receiver = started.events;
    let mut primary_events = Vec::new();
    while let Some(event) = receiver.recv().await {
        primary_events.push(event);
    }
    started.watcher_terminal_processed.notified().await;
    started.llm_watcher_terminal_processed.notified().await;
    started.handle.wait().await;
    assert_completed(&primary_events);

    assert!(primary_events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::SubAgentEvent { .. })));
    let watcher_events = started.watcher_events.lock().expect("watcher events");
    assert!(watcher_events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunCompleted { .. })));
    assert!(
        watcher_events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::SubAgentEvent { .. })),
        "live-attached watcher must receive forwarded SubAgentEvent"
    );
    drop(watcher_events);
    let llm_watcher_events = started
        .llm_watcher_completed_events
        .lock()
        .expect("LLM watcher events");
    assert!(llm_watcher_events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunCompleted { .. })));
    assert!(
        llm_watcher_events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::SubAgentEvent { .. })),
        "live-attached LlmWatcher must receive forwarded SubAgentEvent"
    );
}

#[test]
fn live_attachment_contract_guarantees_first_event() {
    assert!(LIVE_ATTACHMENT_BOUNDARY.contains("start_with_watchers"));
    assert!(LIVE_ATTACHMENT_BOUNDARY.contains("guaranteed"));
}

const CHILD_INJECT: &str = "child-target inject";
const CHILD_STEER: &str = "child-target steer";
const CHILD_CHECKPOINT: &str = "child_checkpoint";

struct RaceFreeGate {
    entered: AtomicU32,
    entered_notify: Notify,
    released: AtomicU32,
    release_notify: Notify,
}

impl RaceFreeGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: AtomicU32::new(0),
            entered_notify: Notify::new(),
            released: AtomicU32::new(0),
            release_notify: Notify::new(),
        })
    }

    async fn hold(&self) {
        self.entered.store(1, Ordering::SeqCst);
        self.entered_notify.notify_waiters();
        while self.released.load(Ordering::SeqCst) == 0 {
            self.release_notify.notified().await;
        }
    }

    async fn wait_entered(&self) {
        while self.entered.load(Ordering::SeqCst) == 0 {
            self.entered_notify.notified().await;
        }
    }

    fn release(&self) {
        self.released.store(1, Ordering::SeqCst);
        self.release_notify.notify_waiters();
    }
}

struct GatedCapturingChildModel {
    calls: AtomicU32,
    gate: Arc<RaceFreeGate>,
    histories: Arc<Mutex<Vec<Vec<Message>>>>,
}

#[async_trait]
impl ModelAdapter for GatedCapturingChildModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "gated-capturing-child"
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
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        self.histories
            .lock()
            .expect("child history lock")
            .push(messages.to_vec());
        if call == 0 {
            self.gate.hold().await;
            return Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "child-checkpoint-1".to_string(),
                    name: CHILD_CHECKPOINT.to_string(),
                    input: json!({}),
                }],
                usage: TokenUsage::default(),
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            });
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("worker complete".to_string())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

#[tokio::test]
async fn child_control_surface_targets_worker_and_awaits_completion() {
    let (_temp, worker) = worker_fixture();
    let child_gate = RaceFreeGate::new();
    let child_histories = Arc::new(Mutex::new(Vec::new()));
    let supervisor_histories = Arc::new(Mutex::new(Vec::new()));
    let first_call_gate = Arc::new(CallGate::default());
    let delegation_call_gate = Arc::new(CallGate::default());

    let supervisor_model: Arc<dyn ModelAdapter> = Arc::new(TwoStageSupervisorModel {
        calls: AtomicU32::new(0),
        first_call_gate: Arc::clone(&first_call_gate),
        delegation_call_gate: Arc::clone(&delegation_call_gate),
        histories: Arc::clone(&supervisor_histories),
        required_messages: vec![],
    });

    let mut child_registry = worker.registry().clone();
    child_registry
        .register(Arc::new(NoopTool::new(CHILD_CHECKPOINT)))
        .expect("register child checkpoint");
    let worker_tool = worker
        .config()
        .clone()
        .as_tool(
            RESEARCH_WORKER_TOOL,
            "Delegate a research request to the Research Pipeline worker.",
        )
        .model(Arc::new(GatedCapturingChildModel {
            calls: AtomicU32::new(0),
            gate: Arc::clone(&child_gate),
            histories: Arc::clone(&child_histories),
        }) as Arc<dyn ModelAdapter>)
        .registry(child_registry)
        .context_mode(ContextMode::Fresh)
        .build()
        .expect("build gated worker tool");

    let mut config =
        orchest::run::AgentConfig::builder("research-supervisor", "research-pipeline/supervisor")
            .system_prompt("Delegate via research_worker.")
            .max_steps(8)
            .build()
            .expect("supervisor config");
    config.runtime.max_steps = 8;
    let mut registry = orchest::tool::registry::ToolRegistry::new();
    registry.register(worker_tool).expect("register worker");
    registry
        .register(Arc::new(NoopTool::new(PROBE_TOOL)))
        .expect("probe");

    let (handle, mut receiver) = AgentRun::start(
        config,
        "parent research question".into(),
        supervisor_model,
        registry,
    );

    first_call_gate.wait_until_entered().await;
    first_call_gate.release();
    delegation_call_gate.wait_until_entered().await;
    delegation_call_gate.release();

    let mut observed_child_run_id = None;
    let mut completion_task = None;
    let mut primary_events = Vec::new();
    while let Some(event) = receiver.recv().await {
        if let RuntimeEvent::SubAgentStarted {
            child_run_id: id, ..
        } = &event
        {
            observed_child_run_id = Some(*id);
            child_gate.wait_entered().await;
            let child = handle
                .child(*id)
                .await
                .expect("resolve public child control surface");
            child.inject_message(CHILD_INJECT);
            child.steer(CHILD_STEER);
            let child_for_wait = handle.child(*id).await.expect("child still registered");
            completion_task = Some(tokio::spawn(async move {
                child_for_wait.wait_completion().await
            }));
            child_gate.release();
        }
        primary_events.push(event);
    }
    assert_completed(&primary_events);

    let child_run_id = observed_child_run_id.expect("SubAgentStarted");
    let resolved = resolve_delegated_worker_target(&handle, child_run_id)
        .await
        .expect("child control surface remains resolvable");
    assert_eq!(resolved.observed_child_run_id, child_run_id);
    assert_eq!(resolved.child_parent_run_id, handle.run_id);

    let outcome = completion_task
        .expect("completion task")
        .await
        .expect("join")
        .expect("child completion without supervisor channel");
    assert!(matches!(
        outcome,
        orchest::run::ChildRunOutcome::Completed { .. }
    ));
    handle.wait().await;

    let child_histories = child_histories.lock().expect("child histories");
    assert!(
        child_histories.iter().flatten().any(|message| {
            message.role == Role::User && message_contains(message, CHILD_INJECT)
        }),
        "child inject must change the child conversation"
    );
    assert!(
        child_histories.iter().flatten().any(|message| {
            message.role == Role::System && message_contains(message, CHILD_STEER)
        }),
        "child steer must change the child conversation"
    );
    drop(child_histories);

    let supervisor_histories = supervisor_histories.lock().expect("supervisor histories");
    assert!(
        supervisor_histories.iter().flatten().all(|message| {
            !message_contains(message, CHILD_INJECT) && !message_contains(message, CHILD_STEER)
        }),
        "child-target control must not alter the supervisor conversation"
    );
}
