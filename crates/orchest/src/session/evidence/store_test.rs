use super::*;
use crate::session::evidence::{EvidenceKind, LocalDecision};

fn record(instance: RunId, at_ms: u64) -> EvidenceRecord {
    EvidenceRecord {
        session_instance: instance,
        worker_id: Uuid::new_v4(),
        run_id: instance,
        step: 0,
        at_ms,
        kind: EvidenceKind::LocalDecision {
            decision: LocalDecision::HookBlocked,
        },
    }
}

#[test]
fn evidence_rows_are_bounded_replayable_and_revocable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("evidence.sqlite");
    let mut database = Database::open(path.clone(), EvidenceLimits::default()).unwrap();
    let instance = RunId::new();
    let id = Uuid::new_v4();
    let current = record(instance, now() as u64);
    database.insert(instance, id, &current).unwrap();
    database.insert(instance, id, &current).unwrap();
    assert_eq!(
        database.read(instance, 0, 64).unwrap().records,
        vec![current.clone()]
    );
    let expired = record(instance, 1);
    database.insert(instance, Uuid::new_v4(), &expired).unwrap();
    assert_eq!(database.read(instance, 0, 64).unwrap().records.len(), 1);
    database
        .conn
        .execute("UPDATE evidence SET payload = X'ff'", [])
        .unwrap();
    assert_eq!(database.read(instance, 0, 64).unwrap().invalid_records, 1);
    database
        .conn
        .execute("UPDATE evidence SET expires = 1", [])
        .unwrap();
    let page = database.read(instance, 0, 64).unwrap();
    assert!(page.records.is_empty());
    assert_eq!(page.expired_records, 1);
    database.revoke(instance).unwrap();
    drop(database);
    let mut reopened = Database::open(path, EvidenceLimits::default()).unwrap();
    reopened.insert(instance, Uuid::new_v4(), &current).unwrap();
    let page = reopened.read(instance, 0, 64).unwrap();
    assert!(page.records.is_empty());
    assert!(page.revoked);
    let next = RunId::new();
    reopened
        .insert(next, Uuid::new_v4(), &record(next, now() as u64))
        .unwrap();
    assert_eq!(reopened.read(next, 0, 64).unwrap().records.len(), 1);
}

#[test]
fn bounded_evidence_page_advances_over_expired_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut database = Database::open(
        dir.path().join("evidence.sqlite"),
        EvidenceLimits::default(),
    )
    .unwrap();
    let instance = RunId::new();
    let current = record(instance, now() as u64);
    let payload = serde_json::to_vec(&current).unwrap();
    let tx = database.conn.transaction().unwrap();
    tx.execute(
        "WITH RECURSIVE positions(n) AS (
             SELECT 1 UNION ALL SELECT n + 1 FROM positions WHERE n < 16384
         )
         INSERT INTO evidence(instance, id, expires, payload)
         SELECT ?1, printf('%d', n), 1, ?2 FROM positions",
        params![instance.0.to_string(), payload],
    )
    .unwrap();
    tx.commit().unwrap();
    database.insert(instance, Uuid::new_v4(), &current).unwrap();

    let page = database.read(instance, 0, 16).unwrap();
    assert!(page.records.is_empty());
    assert_eq!(page.expired_records, 16);
    assert_eq!(page.next_cursor, Some(16));

    let tail = database.read(instance, 16382, 16).unwrap();
    assert_eq!(tail.expired_records, 2);
    assert_eq!(tail.records, vec![current]);
    assert!(tail.next_cursor.is_none());
}

#[cfg(unix)]
#[test]
fn evidence_file_is_created_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private.sqlite");
    drop(Database::open(path.clone(), EvidenceLimits::default()).unwrap());
    let metadata = std::fs::metadata(&path).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o077, 0);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        Database::open(path, EvidenceLimits::default()),
        Err(EvidenceError::UnsafeFilePermissions)
    ));
}
