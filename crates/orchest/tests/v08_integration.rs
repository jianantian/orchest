//! Integration tests for v0.8 features: guardrails, approval, session persistence,
//! resume, and watchers.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::budget::BudgetUsage;
use orchest::events::RuntimeEvent;
use orchest::guardrail::{ToolInputGuardrail, ToolInputGuardrailAction};
use orchest::hook::{Hook, RunHookContext, ToolHookContext};
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun, RunId, Watcher, WatcherAction};
use orchest::session::{InMemorySessionStore, SessionSnapshot, SessionStore};
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError, ToolMetadata, ToolOutput,
    ToolSource,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ── Shared helpers ────────────────────────────────────────────────────────────

fn end_response() -> ModelResponse {
    ModelResponse {
        content: vec![ContentBlock::Text("done".into())],
        usage: TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
        option_adjustments: vec![],
    }
}

async fn collect(mut rx: mpsc::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    let mut v = Vec::new();
    while let Some(e) = rx.recv().await {
        v.push(e);
    }
    v
}

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
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        };
        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        Ok(end_response())
    }
}

// ── Test 1: guardrail_and_approval_coexist ────────────────────────────────────

/// Model that first tries a banned call, then a write (side_effect=true), then ends.
struct TwoCallModel {
    call: AtomicU32,
}

impl TwoCallModel {
    fn new() -> Self {
        Self {
            call: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for TwoCallModel {
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
        let n = self.call.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        };
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        // Count distinct tool-result messages to decide which phase we're in
        let results = messages
            .iter()
            .filter(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
            })
            .count();

        match (n, results) {
            // Phase 1: call banned tool → guardrail rejects it (ToolResult with error back)
            (0, _) => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "c1".into(),
                    name: "write_file".into(),
                    input: json!({"path": "DROP TABLE users"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            // Phase 2: use write_file without banned keyword → needs approval
            (1, _) => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "c2".into(),
                    name: "write_file".into(),
                    input: json!({"path": "/tmp/out.txt", "content": "hello"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            // End
            _ => Ok(ModelResponse {
                content: vec![ContentBlock::Text("all done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
        }
    }
}

struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "writes a file (has side effects)"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: true,
            approval: Approval::WhenRisky,
            execution_mode: orchest::tool::ToolExecutionMode::Normal,
            parallelism: orchest::tool::ToolParallelism::Serial,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Immediate(json!({"written": true})))
    }
}

struct BannedKeywordGuardrail;

#[async_trait]
impl ToolInputGuardrail for BannedKeywordGuardrail {
    async fn check(&self, ctx: &ToolHookContext) -> ToolInputGuardrailAction {
        let s = ctx.tool_input.to_string().to_lowercase();
        if s.contains("drop") || s.contains("delete") {
            return ToolInputGuardrailAction::Reject("banned keyword in input".into());
        }
        ToolInputGuardrailAction::Allow
    }
}

#[tokio::test]
async fn guardrail_and_approval_coexist() {
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("assistant")
        .max_steps(5)
        .build()
        .unwrap()
        .with_tool_input_guardrail(Arc::new(BannedKeywordGuardrail));

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFileTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "write something".into(),
        Arc::new(TwoCallModel::new()),
        registry,
    );

    let mut saw_approval = false;
    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::ApprovalRequested { tool_call, .. } => {
                // Verify the approved call is the SAFE one (not the banned one)
                let path = tool_call
                    .input
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                assert!(
                    !path.contains("DROP"),
                    "banned call must not reach approval"
                );
                saw_approval = true;
                handle.respond_approval(handle.run_id, true).await.unwrap();
            }
            RuntimeEvent::RunFailed { error } => panic!("run failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;

    assert!(
        saw_approval,
        "write_file (side_effect=true) should trigger approval"
    );
}

// ── Test 2: session_resume_with_hooks ─────────────────────────────────────────

struct RunEndCounterHook {
    count: Arc<AtomicU32>,
}

#[async_trait]
impl Hook for RunEndCounterHook {
    async fn on_run_end(&self, _ctx: &RunHookContext) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn session_resume_with_hooks() {
    let store = Arc::new(InMemorySessionStore::new());
    let hook_count = Arc::new(AtomicU32::new(0));
    const SID: &str = "resume-hooks-session";

    // ── First run ──
    let config = AgentConfig::builder("mock/mock")
        .system_prompt("assistant")
        .max_steps(2)
        .session_store(store.clone() as Arc<dyn SessionStore>, SID)
        .build()
        .unwrap()
        .with_hook(Arc::new(RunEndCounterHook {
            count: Arc::clone(&hook_count),
        }));

    let (handle, rx) = AgentRun::start(
        config,
        "hello".into(),
        Arc::new(SimpleEndModel),
        ToolRegistry::new(),
    );
    collect(rx).await;
    handle.wait().await;

    assert_eq!(
        hook_count.load(Ordering::SeqCst),
        1,
        "hook should fire once after first run"
    );

    let snap = store
        .load(SID)
        .await
        .unwrap()
        .expect("snapshot must be saved");
    assert!(!snap.messages.is_empty(), "snapshot must have messages");
    let tokens_after_first = snap.budget_used.tokens_used;

    // ── Resume ──
    let mut snap = snap;
    // Re-attach session store (serde(skip)) and re-attach the hook (also serde(skip))
    snap.active_config = snap
        .active_config
        .with_session_store(store.clone() as Arc<dyn SessionStore>, SID)
        .with_hook(Arc::new(RunEndCounterHook {
            count: Arc::clone(&hook_count),
        }));

    let (handle2, rx2) = AgentRun::resume(snap, Arc::new(SimpleEndModel), ToolRegistry::new());
    collect(rx2).await;
    handle2.wait().await;

    assert_eq!(
        hook_count.load(Ordering::SeqCst),
        2,
        "hook should fire again on resumed run"
    );

    let snap2 = store
        .load(SID)
        .await
        .unwrap()
        .expect("snapshot must be updated after resume");
    assert!(
        snap2.budget_used.tokens_used >= tokens_after_first,
        "resumed snapshot must accumulate token usage"
    );
}

// ── Test 3: session_resume_after_handoff ──────────────────────────────────────

/// Simulate what happens when resuming from a snapshot whose active_config is
/// the post-handoff agent (i.e. the snapshot was captured after a handoff swap).
/// AgentRun::resume must use that config's system_prompt and preserve run_id.
#[tokio::test]
async fn session_resume_after_handoff() {
    let store = Arc::new(InMemorySessionStore::new());
    const SID: &str = "handoff-session";

    // Post-handoff config: represents Agent B that took over via handoff
    let post_handoff_config = AgentConfig::builder("mock/mock")
        .system_prompt("I am Agent B (post-handoff specialist)")
        .max_steps(2)
        .session_store(store.clone() as Arc<dyn SessionStore>, SID)
        .build()
        .unwrap();

    let run_id = RunId::new();

    // Snapshot representing state after handoff: messages include prior context
    let snapshot = SessionSnapshot {
        schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.to_string(),
        session_id: SID.to_string(),
        run_id,
        messages: vec![
            Message {
                role: Role::System,
                content: vec![ContentBlock::Text("I am Agent A (original)".into())],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("help me".into())],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text("handing off to specialist".into())],
            },
        ],
        step: 1,
        budget_used: BudgetUsage::default(),
        active_config: post_handoff_config,
    };

    // Resume from the post-handoff snapshot
    let (handle, rx) = AgentRun::resume(snapshot, Arc::new(SimpleEndModel), ToolRegistry::new());
    assert_eq!(
        handle.run_id, run_id,
        "resumed run_id must match snapshot run_id"
    );

    let events = collect(rx).await;
    handle.wait().await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RunCompleted { .. })),
        "resumed run must complete"
    );

    // Session was updated during resumed run
    let snap2 = store
        .load(SID)
        .await
        .unwrap()
        .expect("snapshot must exist after resume");
    assert_eq!(
        snap2.run_id, run_id,
        "persisted snapshot must retain run_id"
    );
    assert!(
        snap2.active_config.system_prompt.contains("Agent B"),
        "persisted snapshot must reflect post-handoff config"
    );
}

// ── Test 4: watcher_inject_during_tool_loop ───────────────────────────────────

/// Model that calls a tool a fixed number of times then ends. Using a fixed call
/// count (rather than detecting injected messages) avoids timing sensitivity in
/// single-threaded tokio tests where watcher tasks may not be scheduled between
/// every actor message.
struct FixedCallCountModel {
    call: AtomicU32,
    tool_calls: u32,
}

impl FixedCallCountModel {
    fn new(tool_calls: u32) -> Self {
        Self {
            call: AtomicU32::new(0),
            tool_calls,
        }
    }
}

#[async_trait]
impl ModelAdapter for FixedCallCountModel {
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
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        };
        // Yield to allow watcher tasks to be scheduled between model calls.
        tokio::task::yield_now().await;
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }
        if n < self.tool_calls {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: format!("c{n}"),
                    name: "ping".into(),
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
        Ok(ToolOutput::Immediate(json!({"pong": true})))
    }
}

struct CountAndInjectWatcher {
    completions_seen: Arc<AtomicU32>,
}

#[async_trait]
impl Watcher for CountAndInjectWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if matches!(event, RuntimeEvent::ToolCallCompleted { .. }) {
            self.completions_seen.fetch_add(1, Ordering::SeqCst);
            return WatcherAction::Inject("[injected-by-watcher] noted".into());
        }
        WatcherAction::Continue
    }
}

#[tokio::test]
async fn watcher_inject_during_tool_loop() {
    let completions_seen = Arc::new(AtomicU32::new(0));

    let config = AgentConfig::builder("mock/mock")
        .system_prompt("assistant")
        .max_steps(10)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool)).unwrap();

    // Model calls ping 3 times then ends; watcher injects after each completion.
    let (handle, rx) = AgentRun::start(
        config,
        "ping three times".into(),
        Arc::new(FixedCallCountModel::new(3)),
        registry,
    );

    handle
        .attach_watcher(
            Arc::new(CountAndInjectWatcher {
                completions_seen: Arc::clone(&completions_seen),
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
    // Watcher must have seen at least one ToolCallCompleted (exact count depends on
    // when the Subscribe message is processed relative to the first tool call).
    assert!(
        completions_seen.load(Ordering::SeqCst) > 0,
        "watcher must see at least one ToolCallCompleted"
    );
}

// ── Test 5: all_v08_features_combined ─────────────────────────────────────────

/// Verifies guardrail + approval + session persistence + watcher all cooperate.
struct CombinedModel {
    call: AtomicU32,
}

impl CombinedModel {
    fn new() -> Self {
        Self {
            call: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for CombinedModel {
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
        let usage = TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..Default::default()
        };
        // Yield so watcher tasks can be scheduled between model calls.
        tokio::task::yield_now().await;
        if let Some(ref tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        match n {
            // Step 1: try banned keyword → guardrail rejects (no ToolCallCompleted)
            0 => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "c1".into(),
                    name: "write_file".into(),
                    input: json!({"path": "DROP users"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            // Step 2: safe write → approval required → ToolCallCompleted → watcher fires
            1 => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "c2".into(),
                    name: "write_file".into(),
                    input: json!({"path": "/tmp/safe.txt"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            // End
            _ => Ok(ModelResponse {
                content: vec![ContentBlock::Text("combined test done".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
        }
    }
}

struct CountingWatcher {
    tool_completions: Arc<AtomicU32>,
}

#[async_trait]
impl Watcher for CountingWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if matches!(event, RuntimeEvent::ToolCallCompleted { .. }) {
            self.tool_completions.fetch_add(1, Ordering::SeqCst);
            return WatcherAction::Inject("watcher noted completion".into());
        }
        WatcherAction::Continue
    }
}

#[tokio::test]
async fn all_v08_features_combined() {
    let store = Arc::new(InMemorySessionStore::new());
    const SID: &str = "combined-session";
    let tool_completions = Arc::new(AtomicU32::new(0));

    let config = AgentConfig::builder("mock/mock")
        .system_prompt("combined feature assistant")
        .max_steps(10)
        .session_store(store.clone() as Arc<dyn SessionStore>, SID)
        .build()
        .unwrap()
        .with_tool_input_guardrail(Arc::new(BannedKeywordGuardrail));

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFileTool)).unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "process files".into(),
        Arc::new(CombinedModel::new()),
        registry,
    );

    handle
        .attach_watcher(
            Arc::new(CountingWatcher {
                tool_completions: Arc::clone(&tool_completions),
            }),
            512,
        )
        .await;

    let mut saw_approval = false;
    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::ApprovalRequested { .. } => {
                saw_approval = true;
                handle.respond_approval(handle.run_id, true).await.unwrap();
            }
            RuntimeEvent::RunFailed { error } => panic!("run failed: {error}"),
            RuntimeEvent::RunAborted { reason } => {
                panic!("run aborted: {:?}", reason)
            }
            _ => {}
        }
    }
    handle.wait().await;

    // Guardrail: the banned call was rejected (run continued)
    // Approval: the safe write required and received approval
    assert!(saw_approval, "side_effect write_file must require approval");

    // Session: snapshot was persisted
    let snap = store
        .load(SID)
        .await
        .unwrap()
        .expect("snapshot must be saved");
    assert!(!snap.messages.is_empty(), "snapshot must have messages");
    assert!(
        snap.budget_used.tokens_used > 0,
        "snapshot must record token usage"
    );

    // Watcher: the safe write_file's ToolCallCompleted should have fired the watcher
    // (the banned call is rejected before ToolCallStarted so it doesn't emit ToolCallCompleted)
    assert!(
        tool_completions.load(Ordering::SeqCst) > 0,
        "watcher must have seen the approved write_file completion"
    );
}

// ── SQLite session store integration (feature-gated) ─────────────────────────

#[cfg(feature = "sqlite-session")]
mod sqlite_tests {
    use super::*;
    use orchest::session::SqliteSessionStore;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn sqlite_session_store_persist_and_load() {
        let db_file = NamedTempFile::new().expect("tempfile");
        let store = Arc::new(
            SqliteSessionStore::open(db_file.path().to_str().unwrap()).expect("open sqlite store"),
        );
        const SID: &str = "sqlite-test";

        let config = AgentConfig::builder("mock/mock")
            .system_prompt("sqlite test assistant")
            .max_steps(2)
            .session_store(store.clone() as Arc<dyn SessionStore>, SID)
            .build()
            .unwrap();

        let (handle, rx) = AgentRun::start(
            config,
            "hello".into(),
            Arc::new(SimpleEndModel),
            ToolRegistry::new(),
        );
        collect(rx).await;
        handle.wait().await;

        let snap = store.load(SID).await.expect("load").expect("snapshot");
        assert!(!snap.messages.is_empty());
        assert_eq!(snap.session_id, SID);

        // Listing should include this session
        let sessions = store.list().await.expect("list");
        assert!(sessions.contains(&SID.to_string()));

        // Delete and verify gone
        store.delete(SID).await.expect("delete");
        let gone = store.load(SID).await.expect("load after delete");
        assert!(gone.is_none(), "snapshot should be gone after delete");
    }

    #[tokio::test]
    async fn sqlite_session_resume_round_trip() {
        let db_file = NamedTempFile::new().expect("tempfile");
        let store = Arc::new(
            SqliteSessionStore::open(db_file.path().to_str().unwrap()).expect("open sqlite store"),
        );
        const SID: &str = "sqlite-resume";

        let config = AgentConfig::builder("mock/mock")
            .system_prompt("resumable")
            .max_steps(2)
            .session_store(store.clone() as Arc<dyn SessionStore>, SID)
            .build()
            .unwrap();

        let (handle, rx) = AgentRun::start(
            config,
            "start".into(),
            Arc::new(SimpleEndModel),
            ToolRegistry::new(),
        );
        let first_run_id = handle.run_id;
        collect(rx).await;
        handle.wait().await;

        let mut snap = store.load(SID).await.expect("load").expect("snapshot");
        assert_eq!(snap.run_id, first_run_id);

        snap.active_config = snap
            .active_config
            .with_session_store(store.clone() as Arc<dyn SessionStore>, SID);

        let (handle2, rx2) = AgentRun::resume(snap, Arc::new(SimpleEndModel), ToolRegistry::new());
        assert_eq!(handle2.run_id, first_run_id, "resumed run_id must match");
        collect(rx2).await;
        handle2.wait().await;

        let snap2 = store
            .load(SID)
            .await
            .expect("load")
            .expect("snapshot after resume");
        assert_eq!(snap2.run_id, first_run_id);
        assert!(
            snap2.budget_used.tokens_used > 0,
            "tokens must be recorded after resumed run"
        );
    }
}
