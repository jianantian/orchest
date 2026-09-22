use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

use async_trait::async_trait;
use orchest::{
    events::RuntimeEvent,
    model::{
        ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
        RequestOptions, StopReason, TokenUsage, ToolDef,
    },
    run::{AgentRun, SupervisionStrategy},
    tool::{agent_as_tool::ContextMode, ErrorKind, RetryHint},
};
use research_pipeline_demo::{
    fault::{CONTROLLED_FAULT_ABORT_REASON, CONTROLLED_FAULT_CODE},
    supervisor::{build_supervisor, RESEARCH_WORKER_TOOL},
    worker::Worker,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

const ESCALATION_SUMMARY: &str =
    "Supervisor escalation: delegated worker failed; manual recovery is required.";

struct FaultingWorkerModel {
    calls: Arc<AtomicU32>,
}

#[async_trait]
impl ModelAdapter for FaultingWorkerModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "faulting-worker"
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
        // Alternate search → fault so each supervised attempt (including restarts)
        // still completes search_corpus before the controlled fault.
        let (id, name, input) = if call.is_multiple_of(2) {
            (
                format!("search-{}", call / 2 + 1),
                "search_corpus",
                json!({"query": "failure escalation evidence"}),
            )
        } else {
            (
                format!("fault-{}", call / 2 + 1),
                "fault_trigger",
                json!({"reason": "failure escalation evidence"}),
            )
        };
        Ok(ModelResponse {
            content: vec![ContentBlock::ToolUse {
                id,
                name: name.to_string(),
                input,
            }],
            usage: TokenUsage::default(),
            stop_reason: StopReason::ToolUse,
            option_adjustments: vec![],
        })
    }
}

struct EscalatingSupervisorModel {
    calls: Arc<AtomicU32>,
    saw_failed_delegation_result: Arc<AtomicBool>,
}

#[async_trait]
impl ModelAdapter for EscalatingSupervisorModel {
    fn provider_name(&self) -> &str {
        "deterministic"
    }

    fn model_name(&self) -> &str {
        "escalating-supervisor"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        _tx: Option<mpsc::Sender<orchest::model::StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            return Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "delegate-fault-1".to_string(),
                    name: RESEARCH_WORKER_TOOL.to_string(),
                    input: json!({"input": "trigger the controlled fault"}),
                }],
                usage: TokenUsage::default(),
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            });
        }

        let saw_failure = messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|block| match block {
                ContentBlock::ToolResult { content, .. } => {
                    content.pointer("/error/code").and_then(Value::as_str)
                        == Some("SUB_AGENT_RUN_FAILED")
                }
                _ => false,
            });
        self.saw_failed_delegation_result
            .store(saw_failure, Ordering::SeqCst);
        Ok(ModelResponse {
            content: vec![ContentBlock::Text(ESCALATION_SUMMARY.to_string())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

fn worker_fixture() -> (tempfile::TempDir, Worker) {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let corpus = temp.path().join("corpus");
    std::fs::create_dir(&corpus).expect("create corpus");
    std::fs::write(
        corpus.join("source.md"),
        "# Source\nDeterministic failure escalation evidence.",
    )
    .expect("write source fixture");
    let worker = Worker::from_paths(&corpus, temp.path().join("draft.md")).expect("build worker");
    (temp, worker)
}

fn is_terminal(event: &RuntimeEvent) -> bool {
    matches!(
        event,
        RuntimeEvent::RunCompleted { .. }
            | RuntimeEvent::RunFailed { .. }
            | RuntimeEvent::RunAborted { .. }
    )
}

fn contains_restart(event: &RuntimeEvent) -> bool {
    match event {
        RuntimeEvent::RunRestarted { .. } => true,
        RuntimeEvent::SubAgentEvent { event, .. } | RuntimeEvent::ChildRunEvent { event, .. } => {
            contains_restart(event)
        }
        _ => false,
    }
}

fn event_index(
    events: &[RuntimeEvent],
    description: &str,
    predicate: impl Fn(&RuntimeEvent) -> bool,
) -> usize {
    events
        .iter()
        .position(predicate)
        .unwrap_or_else(|| panic!("missing {description} in primary event sequence"))
}

#[tokio::test]
async fn controlled_worker_failure_restarts_once_then_escalates_without_panic() {
    let (temp, worker) = worker_fixture();
    assert!(matches!(
        &worker.config().supervision_strategy,
        SupervisionStrategy::Restart { max_retries: 1 }
    ));
    assert_eq!(worker.config().runtime.repeated_failure.threshold, 1);

    let worker_calls = Arc::new(AtomicU32::new(0));
    let worker_model: Arc<dyn ModelAdapter> = Arc::new(FaultingWorkerModel {
        calls: Arc::clone(&worker_calls),
    });
    let supervisor_calls = Arc::new(AtomicU32::new(0));
    let saw_failed_delegation_result = Arc::new(AtomicBool::new(false));
    let supervisor_model: Arc<dyn ModelAdapter> = Arc::new(EscalatingSupervisorModel {
        calls: Arc::clone(&supervisor_calls),
        saw_failed_delegation_result: Arc::clone(&saw_failed_delegation_result),
    });
    let (config, registry) = build_supervisor(&worker, worker_model, ContextMode::Fresh, true)
        .expect("build fault supervisor");
    assert!(config.system_prompt.contains("escalation summary"));
    assert!(config.system_prompt.contains("without retrying"));
    assert!(
        !config.system_prompt.contains("fault_trigger"),
        "the drill instruction belongs to the worker prompt: a delegation that names \
         fault_trigger reads as a prompt injection to a live watcher"
    );
    let drill_worker =
        Worker::from_paths_fault_drill(&temp.path().join("corpus"), temp.path().join("drill.md"))
            .expect("build fault-drill worker");
    assert!(drill_worker
        .config()
        .system_prompt
        .contains("search_corpus"));
    assert!(drill_worker
        .config()
        .system_prompt
        .contains("fault_trigger"));
    let (handle, mut receiver) = AgentRun::start(
        config,
        "collect controlled failure evidence".into(),
        supervisor_model,
        registry,
    );

    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        let terminal = is_terminal(&event);
        events.push(event);
        if terminal {
            break;
        }
    }
    handle.wait().await;

    assert!(events.last().is_some_and(is_terminal));
    assert_eq!(worker_calls.load(Ordering::SeqCst), 4);
    assert_eq!(supervisor_calls.load(Ordering::SeqCst), 2);

    let search_completed = event_index(&events, "nested search_corpus completion", |event| {
        matches!(
            event,
            RuntimeEvent::SubAgentEvent { event, .. }
                if matches!(
                    event.as_ref(),
                    RuntimeEvent::ToolCallCompleted { tool, .. }
                        if tool == "search_corpus"
                )
        )
    });
    let controlled_fault = event_index(&events, "nested controlled fault failure", |event| {
        matches!(
            event,
            RuntimeEvent::SubAgentEvent { event, .. }
                if matches!(
                    event.as_ref(),
                    RuntimeEvent::ToolCallFailed { tool, .. }
                        if tool == "fault_trigger"
                )
        )
    });
    let worker_run_failed = event_index(&events, "nested worker RunFailed", |event| {
        matches!(
            event,
            RuntimeEvent::SubAgentEvent { event, .. }
                if matches!(event.as_ref(), RuntimeEvent::RunFailed { .. })
        )
    });
    let worker_restarted = event_index(&events, "nested worker RunRestarted", |event| {
        matches!(
            event,
            RuntimeEvent::SubAgentEvent { event, .. }
                if matches!(event.as_ref(), RuntimeEvent::RunRestarted { attempt: 1 })
        )
    });
    let worker_run_failed_final = events
        .iter()
        .enumerate()
        .rev()
        .find(|(_, event)| {
            matches!(
                event,
                RuntimeEvent::SubAgentEvent { event, .. }
                    if matches!(event.as_ref(), RuntimeEvent::RunFailed { .. })
            )
        })
        .map(|(idx, _)| idx)
        .expect("missing final nested worker RunFailed");
    let sub_agent_failed = event_index(&events, "SubAgentFailed lifecycle event", |event| {
        matches!(event, RuntimeEvent::SubAgentFailed { .. })
    });
    let delegation_failed = event_index(&events, "parent AgentAsTool failure", |event| {
        matches!(
            event,
            RuntimeEvent::ToolCallFailed { tool, .. }
                if tool == RESEARCH_WORKER_TOOL
        )
    });
    let supervisor_escalated = event_index(&events, "supervisor escalation completion", |event| {
        matches!(event, RuntimeEvent::RunCompleted { .. })
    });

    assert!(
        search_completed < controlled_fault
            && controlled_fault < worker_run_failed
            && worker_run_failed < worker_restarted
            && worker_restarted < worker_run_failed_final
            && worker_run_failed_final < sub_agent_failed
            && sub_agent_failed < delegation_failed
            && delegation_failed < supervisor_escalated,
        "failure boundaries out of order: search={search_completed}, fault={controlled_fault}, \
         worker_failed={worker_run_failed}, restarted={worker_restarted}, \
         worker_failed_final={worker_run_failed_final}, sub_agent_failed={sub_agent_failed}, \
         delegation_failed={delegation_failed}, escalation={supervisor_escalated}"
    );

    let RuntimeEvent::SubAgentEvent { event, .. } = &events[controlled_fault] else {
        unreachable!("controlled fault index has the asserted wrapper");
    };
    let RuntimeEvent::ToolCallFailed { error, .. } = event.as_ref() else {
        unreachable!("controlled fault index has the asserted event");
    };
    assert_eq!(error.kind, ErrorKind::Fatal);
    assert_eq!(error.retry, RetryHint::Unsafe);
    assert_eq!(error.code.as_deref(), Some(CONTROLLED_FAULT_CODE));

    let RuntimeEvent::SubAgentEvent { event, .. } = &events[worker_run_failed] else {
        unreachable!("worker failure index has the asserted wrapper");
    };
    let RuntimeEvent::RunFailed { error, .. } = event.as_ref() else {
        unreachable!("worker failure index has the asserted event");
    };
    assert_eq!(error, CONTROLLED_FAULT_ABORT_REASON);

    let RuntimeEvent::SubAgentFailed { error, .. } = &events[sub_agent_failed] else {
        unreachable!("sub-agent failure index has the asserted event");
    };
    assert_eq!(error, CONTROLLED_FAULT_ABORT_REASON);

    let RuntimeEvent::ToolCallFailed { error, .. } = &events[delegation_failed] else {
        unreachable!("delegation failure index has the asserted event");
    };
    assert_eq!(error.kind, ErrorKind::Fatal);
    assert_eq!(error.retry, RetryHint::Unsafe);
    assert_eq!(error.code.as_deref(), Some("SUB_AGENT_RUN_FAILED"));

    assert!(saw_failed_delegation_result.load(Ordering::SeqCst));
    let RuntimeEvent::RunCompleted { output, .. } = &events[supervisor_escalated] else {
        unreachable!("escalation index has the asserted event");
    };
    assert_eq!(output, ESCALATION_SUMMARY);
    assert_eq!(events.iter().filter(|e| contains_restart(e)).count(), 1);
    assert!(!events
        .iter()
        .any(|event| matches!(event, RuntimeEvent::HookPanicked { .. })));
}
