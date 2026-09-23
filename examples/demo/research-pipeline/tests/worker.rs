use std::{
    fs,
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use orchest::{
    events::{RunFailureKind, RuntimeEvent},
    hook::{Hook, HookAction, RepeatedFailureHookContext},
    model::{
        ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
        RequestOptions, Role, StopReason, TokenUsage, ToolDef,
    },
    run::{AgentRun, RunId, SupervisionStrategy},
    tool::{Approval, ContextMode, ErrorKind, RetryHint, Tool, ToolContext, ToolOutput},
};
use research_pipeline_demo::events::render_event;
use research_pipeline_demo::fault::{
    ControlledFaultAbortHook, FaultTriggerTool, CONTROLLED_FAULT_ABORT_REASON,
    CONTROLLED_FAULT_CODE,
};
use research_pipeline_demo::worker::{ReadFileTool, SearchCorpusTool, Worker, WriteDraftTool};
use serde_json::json;

fn immediate(output: ToolOutput) -> serde_json::Value {
    match output {
        ToolOutput::Immediate(value) => value,
        other => panic!("expected immediate output, got {other:?}"),
    }
}

#[tokio::test]
async fn deterministic_research_tools_search_read_and_write_owned_paths() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let corpus = temp.path().join("corpus");
    fs::create_dir(&corpus).expect("create corpus");
    let retention = corpus.join("retention.md");
    let support = corpus.join("support.md");
    fs::write(
        &retention,
        "# Retention\nReferral retention improved after onboarding changes.",
    )
    .expect("write retention fixture");
    fs::write(&support, "# Support\nOnboarding dominates support tickets.")
        .expect("write support fixture");

    let search = SearchCorpusTool::from_directory(&corpus).expect("load corpus");
    let search_output = immediate(
        search
            .call_oneshot(json!({"query": "retention onboarding", "top_k": 1}))
            .await
            .expect("search succeeds"),
    );
    let hits = search_output.as_array().expect("search returns an array");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["path"], retention.display().to_string());
    assert_eq!(hits[0]["score"], 2);

    let read = ReadFileTool::new(vec![retention.clone(), support]);
    let read_output = immediate(
        read.call_oneshot(json!({"path": retention.display().to_string()}))
            .await
            .expect("allowed file can be read"),
    );
    assert_eq!(read_output["path"], retention.display().to_string());
    assert!(read_output["content"]
        .as_str()
        .is_some_and(|content| content.contains("Referral retention")));

    let error = read
        .call_oneshot(json!({"path": corpus.join("outside.md")}))
        .await
        .expect_err("unknown file must be rejected");
    assert_eq!(error.kind, ErrorKind::InvalidInput);
    assert_eq!(error.code.as_deref(), Some("PATH_NOT_ALLOWED"));

    let draft_path = temp.path().join("drafts").join("brief.md");
    let write = WriteDraftTool::new(draft_path.clone());
    assert!(write.metadata().side_effect);
    assert!(matches!(write.metadata().approval, Approval::Always));
    let write_output = immediate(
        write
            .call_oneshot(json!({"content": "# Draft\nEvidence-backed."}))
            .await
            .expect("draft write succeeds"),
    );
    assert_eq!(write_output["path"], draft_path.display().to_string());
    assert_eq!(
        fs::read_to_string(draft_path).expect("read written draft"),
        "# Draft\nEvidence-backed."
    );
}

#[tokio::test]
async fn fault_trigger_returns_structured_fatal_unsafe_tool_error() {
    let error = FaultTriggerTool::new()
        .call_oneshot(json!({"reason": "deterministic test"}))
        .await
        .expect_err("fault trigger must return a tool error");

    assert_eq!(error.kind, ErrorKind::Fatal);
    assert_eq!(error.retry, RetryHint::Unsafe);
    assert_eq!(error.code.as_deref(), Some(CONTROLLED_FAULT_CODE));
    assert!(error.message.contains("controlled worker fault"));
}

#[tokio::test]
async fn controlled_fault_hook_aborts_the_matching_repeated_failure() {
    let error = FaultTriggerTool::new()
        .call_oneshot(json!({}))
        .await
        .expect_err("fault trigger must return a tool error");
    let context = RepeatedFailureHookContext {
        run_id: RunId::new(),
        tool_name: "fault_trigger".to_string(),
        error_kind: ErrorKind::Fatal,
        error_history: vec![error],
        count: 1,
    };

    let action = ControlledFaultAbortHook.on_repeated_failure(&context).await;

    assert!(matches!(
        action,
        HookAction::Abort(reason) if reason.contains("controlled worker fault")
    ));
}

struct FaultCallingModel {
    calls: Arc<AtomicU32>,
}

#[async_trait]
impl ModelAdapter for FaultCallingModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "fault-caller"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        _tx: Option<tokio::sync::mpsc::Sender<orchest::model::StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ModelResponse {
            content: vec![ContentBlock::ToolUse {
                id: "fault-1".to_string(),
                name: "fault_trigger".to_string(),
                input: json!({"reason": "worker integration test"}),
            }],
            usage: TokenUsage::default(),
            stop_reason: StopReason::ToolUse,
            option_adjustments: vec![],
        })
    }
}

#[tokio::test]
async fn worker_threshold_and_abort_hook_produce_terminal_run_failed() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let corpus = temp.path().join("corpus");
    fs::create_dir(&corpus).expect("create corpus");
    fs::write(
        corpus.join("source.md"),
        "# Source\nDeterministic evidence.",
    )
    .expect("write source fixture");
    let worker = Worker::from_paths(&corpus, temp.path().join("draft.md")).expect("build worker");

    let names = worker
        .registry()
        .list()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["search_corpus", "read_file", "write_draft", "fault_trigger"]
    );
    let drill = Worker::from_paths_fault_drill(&corpus, temp.path().join("drill.md"))
        .expect("build fault-drill worker");
    let drill_names = drill
        .registry()
        .list()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        drill_names,
        ["search_corpus", "fault_trigger"],
        "the drill exposes only the search step and the fault so the restart \
         sequence stays the run's outcome"
    );
    assert_eq!(worker.config().runtime.repeated_failure.threshold, 1);
    assert!(matches!(
        &worker.config().supervision_strategy,
        SupervisionStrategy::Restart { max_retries: 1 }
    ));

    let calls = Arc::new(AtomicU32::new(0));
    let model: Arc<dyn ModelAdapter> = Arc::new(FaultCallingModel {
        calls: Arc::clone(&calls),
    });
    let (handle, mut receiver) = AgentRun::start(
        worker.config().clone(),
        "trigger the controlled fault".into(),
        model,
        worker.registry().clone(),
    );
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    handle.wait().await;

    // Hook abort maps to RunFailureKind::Other, which is restartable under #251.
    // With max_retries=1 the worker gets one bounded restart (a second model turn),
    // then stays terminal — no infinite loop.
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "Restart retries the eligible Other/hook-abort failure once"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ToolCallFailed { tool, error }
            if tool == "fault_trigger"
                && error.kind == ErrorKind::Fatal
                && error.retry == RetryHint::Unsafe
                && error.code.as_deref() == Some(CONTROLLED_FAULT_CODE)
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::RunRestarted { attempt: 1 })),
        "eligible RunFailed must emit RunRestarted before the retry"
    );
    let run_failed = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                RuntimeEvent::RunFailed {
                    error,
                    kind: RunFailureKind::Other
                } if error == CONTROLLED_FAULT_ABORT_REASON
            )
        })
        .count();
    assert_eq!(
        run_failed, 2,
        "initial abort plus post-restart abort must both emit RunFailed"
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::RunCompleted { .. })));
}

struct CapturingModel {
    calls: Arc<Mutex<Vec<Vec<Message>>>>,
}

#[async_trait]
impl ModelAdapter for CapturingModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "context-capture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        _tx: Option<tokio::sync::mpsc::Sender<orchest::model::StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        self.calls
            .lock()
            .expect("capture lock")
            .push(messages.to_vec());
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".to_string())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

fn message_text(message: &Message) -> &str {
    match message.content.as_slice() {
        [ContentBlock::Text(text)] => text,
        other => panic!("expected one text block, got {other:?}"),
    }
}

fn worker_fixture() -> (tempfile::TempDir, Worker) {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let corpus = temp.path().join("corpus");
    fs::create_dir(&corpus).expect("create corpus");
    fs::write(
        corpus.join("source.md"),
        "# Source\nDeterministic evidence.",
    )
    .expect("write source fixture");
    let worker = Worker::from_paths(&corpus, temp.path().join("draft.md")).expect("build worker");
    (temp, worker)
}

#[tokio::test]
async fn fresh_context_inherits_no_parent_messages() {
    let (_temp, worker) = worker_fixture();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let model: Arc<dyn ModelAdapter> = Arc::new(CapturingModel {
        calls: Arc::clone(&calls),
    });
    let tool = worker
        .as_tool(
            "research_worker",
            "delegated worker",
            model,
            ContextMode::Fresh,
        )
        .expect("build fresh worker tool");
    let parent_messages = vec![
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text("parent one".to_string())],
        },
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("parent two".to_string())],
        },
    ];

    tool.execute(
        json!({"input": "child request"}),
        &ToolContext {
            parent_messages,
            ..ToolContext::oneshot()
        },
    )
    .await
    .expect("fresh child run succeeds");

    let calls = calls.lock().expect("capture lock");
    let messages = calls.first().expect("one child model call");
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0].role, Role::System));
    assert!(matches!(messages[1].role, Role::User));
    assert_eq!(message_text(&messages[1]), "child request");
}

#[tokio::test]
async fn fork_context_inherits_only_the_bounded_parent_tail() {
    let (_temp, worker) = worker_fixture();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let model: Arc<dyn ModelAdapter> = Arc::new(CapturingModel {
        calls: Arc::clone(&calls),
    });
    let depth = NonZeroUsize::new(2).expect("non-zero depth");
    let tool = worker
        .as_tool(
            "research_worker",
            "delegated worker",
            model,
            ContextMode::Fork { depth },
        )
        .expect("build forked worker tool");
    let parent_messages = ["discarded", "inherited one", "inherited two"]
        .into_iter()
        .map(|text| Message {
            role: Role::User,
            content: vec![ContentBlock::Text(text.to_string())],
        })
        .collect();

    tool.execute(
        json!({"input": "child request"}),
        &ToolContext {
            parent_messages,
            ..ToolContext::oneshot()
        },
    )
    .await
    .expect("forked child run succeeds");

    let calls = calls.lock().expect("capture lock");
    let messages = calls.first().expect("one child model call");
    assert_eq!(messages.len(), 4);
    assert!(matches!(messages[0].role, Role::System));
    assert_eq!(message_text(&messages[1]), "inherited one");
    assert_eq!(message_text(&messages[2]), "inherited two");
    assert_eq!(message_text(&messages[3]), "child request");
}

#[tokio::test]
async fn fork_context_without_parent_history_returns_clear_error() {
    let (_temp, worker) = worker_fixture();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let model: Arc<dyn ModelAdapter> = Arc::new(CapturingModel {
        calls: Arc::clone(&calls),
    });
    let depth = NonZeroUsize::new(2).expect("non-zero depth");
    let tool = worker
        .as_tool(
            "research_worker",
            "delegated worker",
            model,
            ContextMode::Fork { depth },
        )
        .expect("build forked worker tool");

    let error = tool
        .call_oneshot(json!({"input": "child request"}))
        .await
        .expect_err("empty parent history must not fall back to fresh");

    assert_eq!(error.kind, ErrorKind::InvalidInput);
    assert_eq!(error.code.as_deref(), Some("EMPTY_PARENT_CONTEXT"));
    assert!(error.message.contains("requires parent message history"));
    assert!(calls.lock().expect("capture lock").is_empty());
}

#[test]
fn event_renderer_identifies_model_tools_results_and_terminal_status() {
    let parent_run_id = RunId::new();
    let child_run_id = RunId::new();
    let rendered = [
        render_event(&RuntimeEvent::ModelCallStarted { step: 2 }),
        render_event(&RuntimeEvent::ModelCallCompleted {
            tokens: TokenUsage {
                input_tokens: 7,
                output_tokens: 3,
                ..TokenUsage::default()
            },
            option_adjustments: vec![],
        }),
        render_event(&RuntimeEvent::SubAgentEvent {
            parent_run_id,
            child_run_id,
            event: Box::new(RuntimeEvent::ToolCallStarted {
                tool: "search_corpus".to_string(),
                metadata: orchest::tool::ToolMetadata::default(),
                input: json!({"query": "retention"}),
            }),
        }),
        render_event(&RuntimeEvent::SubAgentEvent {
            parent_run_id,
            child_run_id,
            event: Box::new(RuntimeEvent::ToolCallCompleted {
                tool: "search_corpus".to_string(),
                output: json!([{"path": "source.md"}]),
                duration: Duration::from_millis(5),
            }),
        }),
        render_event(&RuntimeEvent::RunCompleted {
            output: json!("done"),
            stop_reason: StopReason::EndTurn,
        }),
        render_event(&RuntimeEvent::RunFailed {
            error: "controlled worker fault".to_string(),
            kind: orchest::events::RunFailureKind::Other,
        }),
    ];

    assert_eq!(rendered[0], "[run] model turn started: step 2");
    assert_eq!(
        rendered[1],
        "[run] model turn completed: 7 input tokens, 3 output tokens"
    );
    assert_eq!(rendered[2], "[worker] tool call started: search_corpus");
    assert_eq!(rendered[3], "[worker] tool result: search_corpus (5ms)");
    assert_eq!(rendered[4], "[run] terminal status: completed (EndTurn)");
    assert_eq!(
        rendered[5],
        "[run] terminal status: failed (controlled worker fault)"
    );
}
