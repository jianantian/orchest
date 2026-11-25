use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{AgentConfig, AgentRun, BackoffStrategy, EventReceiver, RetryPolicy, RunHandle};
use crate::hook::{Hook, ModelHookAction, ModelHookContext};
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
        _tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let mut seen = self.seen.lock().unwrap();
        seen.push(json!([messages, tools, options]));
        if self.fail_first && seen.len() == 1 {
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
