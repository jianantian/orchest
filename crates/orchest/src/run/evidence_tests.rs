use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{AgentConfig, AgentRun, BackoffStrategy, EventReceiver, RetryPolicy, RunHandle};
use crate::hook::{Hook, HookAction, ModelHookAction, ModelHookContext};
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, Role, StopReason, StreamEvent, TokenUsage, ToolDef,
};
use crate::session::evidence::{
    reconstruct_input, EvidenceContentPolicy, EvidenceKind, EvidenceLimits, EvidenceSession,
    EvidenceStore,
};
use crate::session::{InMemorySessionStore, SessionStore};
use crate::tool::registry::ToolRegistry;

#[derive(Default)]
struct FakeEvidenceModel {
    seen: Mutex<Vec<Value>>,
    fail_first: bool,
    transport: bool,
}

#[async_trait::async_trait]
impl ModelAdapter for FakeEvidenceModel {
    fn provider_name(&self) -> &str {
        "fake"
    }
    fn model_name(&self) -> &str {
        "test"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let call = {
            let mut seen = self.seen.lock().unwrap();
            seen.push(json!([messages, tools, options]));
            seen.len()
        };
        if self.transport {
            crate::session::evidence::mark_request_sent();
            if let Some(tx) = tx {
                tx.send(StreamEvent::Text {
                    delta: "visible prefix".into(),
                })
                .await
                .unwrap();
            }
        }
        if self.fail_first && call == 1 {
            return Err(ModelError::internal("retry this call", "timeout"));
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("answer".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

struct AttemptHook(AtomicUsize);

#[async_trait::async_trait]
impl Hook for AttemptHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        let attempt = self.0.fetch_add(1, Ordering::Relaxed);
        ctx.messages.push(Message {
            role: Role::System,
            content: vec![ContentBlock::Text(format!("ephemeral-{attempt}"))],
        });
        ModelHookAction::Continue
    }
}

pub(super) async fn drain(handle: RunHandle, mut events: EventReceiver) {
    while events.recv().await.is_some() {}
    handle.wait().await;
}

fn config(storage: Arc<dyn SessionStore>, capture: EvidenceSession) -> AgentConfig {
    AgentConfig::builder("local", "test")
        .retry_policy(RetryPolicy {
            max_retries: 1,
            backoff: BackoffStrategy::Fixed(Duration::ZERO),
        })
        .build()
        .unwrap()
        .with_session_store(storage, "local")
        .with_session_evidence(capture.with_content_policy(EvidenceContentPolicy::new(|_, _| true)))
}

#[tokio::test]
async fn evidence_rebuilds_actual_adapter_input_after_retry() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("evidence.sqlite");
    let database = EvidenceStore::open(&path, EvidenceLimits::default())
        .await
        .unwrap();
    let storage = Arc::new(InMemorySessionStore::new());
    let capture = database.bind(storage.clone(), "local");
    let model = Arc::new(FakeEvidenceModel {
        fail_first: true,
        ..Default::default()
    });
    let cfg = config(storage.clone(), capture.clone()).with_hook(Arc::new(AttemptHook(0.into())));
    let (handle, events) =
        AgentRun::start(cfg, "prompt".into(), model.clone(), ToolRegistry::new());
    drain(handle, events).await;

    capture.read(None, 64).await.unwrap();
    let reopened = EvidenceStore::open(path, EvidenceLimits::default())
        .await
        .unwrap();
    let page = reopened
        .bind(storage.clone(), "local")
        .read(None, 64)
        .await
        .unwrap();
    let reconstructed: Vec<_> = page
        .records
        .iter()
        .filter_map(|record| match record.kind {
            EvidenceKind::AttemptStarted { attempt_id, .. } => {
                let input = reconstruct_input(&page.records, attempt_id).unwrap();
                Some(json!([input.messages, input.tools, input.options]))
            }
            _ => None,
        })
        .collect();
    assert_eq!(reconstructed.len(), 2);
    assert_eq!(reconstructed, *model.seen.lock().unwrap());
    assert_ne!(reconstructed[0][0], reconstructed[1][0]);
    assert!(
        !serde_json::to_string(&storage.read("local", 1).await.unwrap())
            .unwrap()
            .contains("ephemeral-")
    );
}

#[tokio::test]
async fn evidence_withholds_restricted_input_and_revocation_survives_reopen() {
    use crate::session::evidence::{CapturedField, EvidenceField, Omission};

    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("evidence.sqlite");
    let database = EvidenceStore::open(&path, EvidenceLimits::default())
        .await
        .unwrap();
    let storage = Arc::new(InMemorySessionStore::new());
    let capture = database.bind(storage.clone(), "local");
    let cfg = config(storage.clone(), capture.clone());
    let (handle, events) = AgentRun::start(
        cfg.clone(),
        "Bearer credential-fixture".into(),
        Arc::new(FakeEvidenceModel::default()),
        ToolRegistry::new(),
    );
    drain(handle, events).await;
    let page = capture.read(None, 64).await.unwrap();
    assert!(page.records.iter().any(|record| matches!(
        record.kind,
        EvidenceKind::Field {
            field: EvidenceField::Messages,
            capture: CapturedField::Omitted(Omission::Restricted),
            ..
        }
    )));
    assert!(!serde_json::to_string(&page.records)
        .unwrap()
        .contains("credential-fixture"));
    capture.revoke().await.unwrap();
    let reopened = EvidenceStore::open(path, EvidenceLimits::default())
        .await
        .unwrap();
    assert!(reopened
        .bind(storage, "local")
        .read(None, 64)
        .await
        .unwrap()
        .records
        .is_empty());
}

struct RewriteResponse;

#[async_trait::async_trait]
impl Hook for RewriteResponse {
    async fn after_model(&self, ctx: &mut ModelHookContext) -> HookAction {
        ctx.response = Some(vec![ContentBlock::Text("accepted answer".into())]);
        HookAction::Continue
    }
}

#[tokio::test]
async fn evidence_distinguishes_retry_stream_and_confirmed_assistant_event() {
    use crate::session::evidence::{
        CapturedField, EvidenceEventKind, EvidenceField, HookDisposition,
    };

    let folder = tempfile::tempdir().unwrap();
    let database = EvidenceStore::open(
        folder.path().join("evidence.sqlite"),
        EvidenceLimits::default(),
    )
    .await
    .unwrap();
    let storage = Arc::new(InMemorySessionStore::new());
    let capture = database.bind(storage.clone(), "local");
    let cfg = config(storage, capture.clone()).with_hook(Arc::new(RewriteResponse));
    let (handle, events) = AgentRun::start(
        cfg,
        "prompt".into(),
        Arc::new(FakeEvidenceModel {
            fail_first: true,
            transport: true,
            ..Default::default()
        }),
        ToolRegistry::new(),
    );
    drain(handle, events).await;

    let records = capture.read(None, 64).await.unwrap().records;
    let failed = records
        .iter()
        .find_map(|record| match record.kind {
            EvidenceKind::AdapterFailed { attempt_id, .. } => Some(attempt_id),
            _ => None,
        })
        .unwrap();
    assert!(records.iter().any(
        |r| matches!(r.kind, EvidenceKind::TransportSent { attempt_id } if attempt_id == failed)
    ));
    assert!(records
        .iter()
        .any(|r| matches!(&r.kind, EvidenceKind::StreamText {
        attempt_id, capture: CapturedField::Exact(delta), ..
    } if *attempt_id == failed && delta == "visible prefix")));
    assert!(!records.iter().any(
        |r| matches!(r.kind, EvidenceKind::SessionEvent { attempt_id, .. } if attempt_id == failed)
    ));

    let reference = records
        .iter()
        .find(|r| {
            matches!(r.kind, EvidenceKind::SessionEvent { event, .. }
        if event.kind == EvidenceEventKind::AssistantMessage)
        })
        .unwrap();
    let watermark = records
        .iter()
        .rev()
        .find(|r| matches!(r.kind, EvidenceKind::SessionDurableThrough { .. }))
        .unwrap();
    assert!(capture.verify_event(reference, watermark).await.unwrap());
    let EvidenceKind::SessionEvent {
        attempt_id: accepted,
        ..
    } = reference.kind
    else {
        unreachable!()
    };
    assert!(records
        .iter()
        .any(|r| matches!(r.kind, EvidenceKind::AfterModel {
        attempt_id, disposition: HookDisposition::Rewritten,
    } if attempt_id == accepted)));
    for (field, expected) in [
        (EvidenceField::Response, "answer"),
        (EvidenceField::CommittedResponse, "accepted answer"),
    ] {
        assert!(records
            .iter()
            .any(|r| matches!(&r.kind, EvidenceKind::Field {
            attempt_id, field: actual, capture: CapturedField::Exact(content),
        } if *attempt_id == accepted && *actual == field
            && *content == json!([ContentBlock::Text(expected.into())]))));
    }
    let mut wrong_worker = watermark.clone();
    wrong_worker.worker_id = uuid::Uuid::new_v4();
    assert!(!capture
        .verify_event(reference, &wrong_worker)
        .await
        .unwrap());
}
