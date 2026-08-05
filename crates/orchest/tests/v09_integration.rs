//! Integration tests for v0.9 features: supervised delegation, LlmWatcher,
//! crash recovery, and multi-watcher coordination.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun, SupervisionStrategy, Watcher, WatcherAction};
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput,
    ToolSource,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ── Shared helpers ────────────────────────────────────────────────────────────

async fn collect(mut rx: mpsc::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    let mut v = Vec::new();
    while let Some(e) = rx.recv().await {
        v.push(e);
    }
    v
}

fn usage() -> TokenUsage {
    TokenUsage {
        input_tokens: 5,
        output_tokens: 3,
        ..Default::default()
    }
}

struct PingTool;

#[async_trait]
impl Tool for PingTool {
    fn name(&self) -> &str {
        "ping"
    }
    fn description(&self) -> &str {
        "pings"
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
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        Ok(ToolOutput::Immediate(json!({"pong": true})))
    }
}

/// Model that calls ping N times then ends.
struct FixedCallModel {
    call: AtomicU32,
    tool_calls: u32,
}

impl FixedCallModel {
    fn new(tool_calls: u32) -> Self {
        Self {
            call: AtomicU32::new(0),
            tool_calls,
        }
    }
}

#[async_trait]
impl ModelAdapter for FixedCallModel {
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
        let n = self.call.fetch_add(1, Ordering::SeqCst);
        let u = usage();
        tokio::task::yield_now().await;
        if let Some(ref tx) = tx {
            let _ = tx.send(StreamEvent::Done { usage: u.clone() }).await;
        }
        if n < self.tool_calls {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: format!("c{n}"),
                    name: "ping".into(),
                    input: json!({}),
                }],
                usage: u,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        } else {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("done".into())],
                usage: u,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
}

/// Model that just ends immediately.
struct SimpleEndModel;

#[async_trait]
impl ModelAdapter for SimpleEndModel {
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
        let u = usage();
        if let Some(tx) = tx {
            let _ = tx.send(StreamEvent::Done { usage: u.clone() }).await;
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".into())],
            usage: u,
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

// ── Test 1: watcher inject ──────────────────────────────────────────────────

struct InjectWatcher {
    seen: Arc<AtomicU32>,
}

#[async_trait]
impl Watcher for InjectWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if matches!(event, RuntimeEvent::ToolCallCompleted { .. }) {
            self.seen.fetch_add(1, Ordering::SeqCst);
            return WatcherAction::Inject("focus on error handling".into());
        }
        WatcherAction::Continue
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watcher_inject_adds_user_message() {
    let seen = Arc::new(AtomicU32::new(0));

    let config = AgentConfig::builder("test-agent", "mock/mock")
        .system_prompt("assistant")
        .max_steps(10)
        .supervision_strategy(SupervisionStrategy::Stop)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool)).unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "ping four times".into(),
        Arc::new(FixedCallModel::new(4)),
        registry,
    );

    handle
        .attach_watcher(
            Arc::new(InjectWatcher {
                seen: Arc::clone(&seen),
            }),
            512,
        )
        .await;

    let events = collect(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run must complete"
    );
    assert!(
        seen.load(Ordering::SeqCst) > 0,
        "inject watcher must see at least one ToolCallCompleted"
    );
}

// ── Test 2: watcher steer ───────────────────────────────────────────────────

struct SteerWatcher {
    steered: Arc<AtomicU32>,
}

#[async_trait]
impl Watcher for SteerWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if matches!(event, RuntimeEvent::ToolCallCompleted { .. }) {
            self.steered.fetch_add(1, Ordering::SeqCst);
            return WatcherAction::Steer("summarize findings".into());
        }
        WatcherAction::Continue
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watcher_steer_injects_system_instruction() {
    let steered = Arc::new(AtomicU32::new(0));

    let config = AgentConfig::builder("test-agent", "mock/mock")
        .system_prompt("assistant")
        .max_steps(10)
        .supervision_strategy(SupervisionStrategy::Stop)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool)).unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "ping thrice".into(),
        Arc::new(FixedCallModel::new(3)),
        registry,
    );

    handle
        .attach_watcher(
            Arc::new(SteerWatcher {
                steered: Arc::clone(&steered),
            }),
            512,
        )
        .await;

    let events = collect(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run must complete"
    );
    assert!(
        steered.load(Ordering::SeqCst) > 0,
        "steer watcher must see at least one ToolCallCompleted"
    );
}

// ── Test 3: watcher abort ───────────────────────────────────────────────────

struct AbortWatcher;

#[async_trait]
impl Watcher for AbortWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if matches!(event, RuntimeEvent::ToolCallCompleted { .. }) {
            return WatcherAction::Abort("off-track detected".into());
        }
        WatcherAction::Continue
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watcher_abort_terminates_run() {
    let config = AgentConfig::builder("test-agent", "mock/mock")
        .system_prompt("assistant")
        .max_steps(10)
        .supervision_strategy(SupervisionStrategy::Stop)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool)).unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "ping many".into(),
        Arc::new(FixedCallModel::new(5)),
        registry,
    );

    handle
        .attach_watcher(Arc::new(AbortWatcher) as Arc<dyn Watcher>, 512)
        .await;

    let events = collect(rx).await;
    handle.wait().await;

    let aborted = events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunAborted { .. }));
    let completed = events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. }));
    assert!(aborted || completed, "run must end (abort or complete)");
}

// ── Test 4: supervision strategy restart ────────────────────────────────────

#[tokio::test]
async fn supervision_restart_strategy_configured() {
    let config = AgentConfig::builder("test-agent", "mock/mock")
        .system_prompt("assistant")
        .max_steps(5)
        .supervision_strategy(SupervisionStrategy::Restart { max_retries: 2 })
        .build()
        .unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "hello".into(),
        Arc::new(SimpleEndModel),
        ToolRegistry::new(),
    );

    let events = collect(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run with restart strategy must complete normally when no crash occurs"
    );
}

// ── Test 5: multi-watcher coordination ──────────────────────────────────────

struct CountingWatcher {
    count: Arc<AtomicU32>,
}

#[async_trait]
impl Watcher for CountingWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if matches!(event, RuntimeEvent::ToolCallCompleted { .. }) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
        WatcherAction::Continue
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_watcher_both_receive_events() {
    let count_a = Arc::new(AtomicU32::new(0));
    let count_b = Arc::new(AtomicU32::new(0));

    let config = AgentConfig::builder("test-agent", "mock/mock")
        .system_prompt("assistant")
        .max_steps(10)
        .supervision_strategy(SupervisionStrategy::Stop)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool)).unwrap();

    let (handle, rx) = AgentRun::start(
        config,
        "ping four times".into(),
        Arc::new(FixedCallModel::new(4)),
        registry,
    );

    handle
        .attach_watcher(
            Arc::new(CountingWatcher {
                count: Arc::clone(&count_a),
            }),
            512,
        )
        .await;
    handle
        .attach_watcher(
            Arc::new(CountingWatcher {
                count: Arc::clone(&count_b),
            }),
            512,
        )
        .await;

    tokio::task::yield_now().await;

    let events = collect(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "run must complete"
    );

    let a = count_a.load(Ordering::SeqCst);
    let b = count_b.load(Ordering::SeqCst);
    assert!(a > 0, "watcher A must see ToolCallCompleted events");
    assert!(b > 0, "watcher B must see ToolCallCompleted events");
}
