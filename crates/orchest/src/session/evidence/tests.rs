use std::sync::Arc;

use crate::run::RunId;
use crate::session::SessionEventKind;
use uuid::Uuid;

use super::*;
use crate::session::{InMemorySessionStore, SessionLog, SessionStore};

#[tokio::test]
async fn evidence_cannot_confirm_a_reused_unflushed_sequence() {
    let folder = tempfile::tempdir().unwrap();
    let database = EvidenceStore::open(
        folder.path().join("evidence.sqlite"),
        EvidenceLimits::default(),
    )
    .await
    .unwrap();
    let store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
    let scoped = database.bind(store.clone(), "local");
    let run_id = RunId::new();
    let mut first_log = SessionLog::new();
    first_log.append(SessionEventKind::RunStarted {
        run_id,
        agent_name: "test".into(),
    });
    store.append("local", first_log.events()).await.unwrap();

    let old = scoped
        .bind_run(&store, "local", first_log.events().first())
        .unwrap();
    let new = scoped
        .bind_run(&store, "local", first_log.events().first())
        .unwrap();
    let orphan = first_log
        .append(SessionEventKind::AssistantMessage { content: vec![] })
        .clone();
    let reference = EvidenceEventRef::from_event(&orphan).unwrap();
    old.record(
        run_id,
        0,
        EvidenceKind::SessionEvent {
            attempt_id: Uuid::new_v4(),
            event: reference,
        },
    );

    // An indistinguishable payload, timestamp and sequence in the later
    // worker still cannot serve as proof of the old worker's append.
    store.append("local", &[orphan]).await.unwrap();
    store.flush("local").await.unwrap();
    new.record(
        run_id,
        0,
        EvidenceKind::SessionDurableThrough { seq: reference.seq },
    );
    let records = scoped.read(None, 64).await.unwrap().records;
    assert_eq!(records.len(), 2);
    assert!(!scoped.verify_event(&records[0], &records[1]).await.unwrap());

    store.delete("local").await.unwrap();
    let mut replacement = SessionLog::new();
    replacement.append(SessionEventKind::RunStarted {
        run_id: RunId::new(),
        agent_name: "test".into(),
    });
    store.append("local", replacement.events()).await.unwrap();
    assert!(scoped.read(None, 64).await.unwrap().records.is_empty());
}
