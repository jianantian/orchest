use crate::session::SessionStore;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use rusqlite::{params, Connection, ErrorCode, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::{EvidenceError, EvidenceLimits, EvidencePage, EvidenceRecord, EvidenceSession};
use crate::run::RunId;

pub(crate) const EVIDENCE_APPLICATION_ID: i32 = 0x4f45_5644;
const SCHEMA: i32 = 1;
pub(super) const MAX_PAGE_RECORDS: usize = 64;

#[derive(Clone)]
pub struct EvidenceStore {
    queue: mpsc::Sender<Command>,
    limits: EvidenceLimits,
    drops: Arc<AtomicU64>,
}

enum Command {
    Record {
        instance: RunId,
        id: Uuid,
        record: EvidenceRecord,
    },
    Read {
        instance: RunId,
        cursor: i64,
        limit: usize,
        purge: bool,
        reply: oneshot::Sender<Result<EvidencePage, EvidenceError>>,
    },
    Contains {
        instance: RunId,
        payloads: [Vec<u8>; 2],
        reply: oneshot::Sender<Result<bool, EvidenceError>>,
    },
    Revoke {
        instance: RunId,
        reply: oneshot::Sender<Result<(), EvidenceError>>,
    },
}

impl EvidenceStore {
    /// Opens a dedicated local evidence database. Use a host-controlled path.
    pub async fn open(
        path: impl AsRef<Path>,
        limits: EvidenceLimits,
    ) -> Result<Self, EvidenceError> {
        if !(1..=1024).contains(&limits.queue_records)
            || !(256..=1024 * 1024).contains(&limits.record_bytes)
            || !(128 * 1024..=1024 * 1024 * 1024).contains(&limits.database_bytes)
            || limits.retention.as_millis() == 0
            || limits.retention > Duration::from_secs(365 * 24 * 60 * 60)
        {
            return Err(EvidenceError::InvalidLimits);
        }
        let path = path.as_ref().to_owned();
        let database = tokio::task::spawn_blocking(move || Database::open(path, limits))
            .await
            .map_err(|_| EvidenceError::Unavailable)??;
        let (queue, receiver) = mpsc::channel(limits.queue_records);
        let drops = Arc::new(AtomicU64::new(0));
        tokio::spawn(run_writer(database, receiver, Arc::clone(&drops)));
        Ok(Self {
            queue,
            limits,
            drops,
        })
    }

    /// Bind this capability to the exact SessionStore used by AgentConfig.
    pub fn bind(
        &self,
        store: Arc<dyn SessionStore>,
        session_id: impl Into<String>,
    ) -> EvidenceSession {
        EvidenceSession {
            database: self.clone(),
            store,
            session_id: session_id.into(),
            content: None,
        }
    }

    pub(super) fn record_with(&self, instance: RunId, record: impl FnOnce() -> EvidenceRecord) {
        match self.queue.try_reserve() {
            Ok(permit) => permit.send(Command::Record {
                instance,
                id: Uuid::new_v4(),
                record: record(),
            }),
            Err(_) => {
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn record_bytes(&self) -> usize {
        self.limits.record_bytes
    }

    pub(super) async fn read(
        &self,
        instance: RunId,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<EvidencePage, EvidenceError> {
        self.read_page(instance, cursor, limit.clamp(1, MAX_PAGE_RECORDS), true)
            .await
    }

    pub(super) async fn read_bounded(
        &self,
        instance: RunId,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<EvidencePage, EvidenceError> {
        self.read_page(instance, cursor, limit, false).await
    }

    async fn read_page(
        &self,
        instance: RunId,
        cursor: Option<i64>,
        limit: usize,
        purge: bool,
    ) -> Result<EvidencePage, EvidenceError> {
        let (reply, response) = oneshot::channel();
        self.queue
            .send(Command::Read {
                instance,
                cursor: cursor.unwrap_or(0).max(0),
                limit,
                purge,
                reply,
            })
            .await
            .map_err(|_| EvidenceError::Unavailable)?;
        let mut page = response.await.map_err(|_| EvidenceError::Unavailable)??;
        page.observed_drops = self.drops.load(Ordering::Relaxed);
        Ok(page)
    }

    pub(super) async fn contains(
        &self,
        instance: RunId,
        records: [&EvidenceRecord; 2],
    ) -> Result<bool, EvidenceError> {
        let payloads = [
            encode(records[0], self.limits.record_bytes)?,
            encode(records[1], self.limits.record_bytes)?,
        ];
        let (reply, response) = oneshot::channel();
        self.queue
            .send(Command::Contains {
                instance,
                payloads,
                reply,
            })
            .await
            .map_err(|_| EvidenceError::Unavailable)?;
        response.await.map_err(|_| EvidenceError::Unavailable)?
    }

    pub(super) async fn revoke(&self, instance: RunId) -> Result<(), EvidenceError> {
        let (reply, response) = oneshot::channel();
        self.queue
            .send(Command::Revoke { instance, reply })
            .await
            .map_err(|_| EvidenceError::Unavailable)?;
        response.await.map_err(|_| EvidenceError::Unavailable)?
    }
}

struct Database {
    conn: Connection,
    limits: EvidenceLimits,
}

async fn run_writer(
    mut database: Database,
    mut receiver: mpsc::Receiver<Command>,
    drops: Arc<AtomicU64>,
) {
    let interval = database
        .limits
        .retention
        .clamp(Duration::from_secs(1), Duration::from_secs(60));
    let mut maintenance = tokio::time::interval(interval);
    loop {
        let command = tokio::select! {
            _ = maintenance.tick() => None,
            command = receiver.recv() => {
                let Some(command) = command else { break };
                Some(command)
            },
        };
        let drops = Arc::clone(&drops);
        let Ok(next) = tokio::task::spawn_blocking(move || {
            match command {
                Some(command) => database.process(command, &drops),
                None => {
                    let _ = database.purge();
                }
            }
            database
        })
        .await
        else {
            break;
        };
        database = next;
    }
}

impl Database {
    fn open(path: PathBuf, limits: EvidenceLimits) -> Result<Self, EvidenceError> {
        #[cfg(unix)]
        prepare_private_file(&path)?;
        let mut conn = Connection::open(path).map_err(storage)?;
        conn.busy_timeout(Duration::from_millis(250))
            .map_err(storage)?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let id: i32 = tx
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(storage)?;
        let version: i32 = tx
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage)?;
        match (id, version) {
            (0, 0) => {
                let empty: bool = tx
                    .query_row(
                        "SELECT NOT EXISTS (SELECT 1 FROM sqlite_master)",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(storage)?;
                if !empty {
                    return Err(EvidenceError::Schema);
                }
                tx.execute_batch(
                    "CREATE TABLE evidence (
                        cursor INTEGER PRIMARY KEY AUTOINCREMENT,
                        instance TEXT NOT NULL, id TEXT NOT NULL UNIQUE,
                        expires INTEGER NOT NULL, payload BLOB NOT NULL
                    );
                    CREATE INDEX evidence_instance ON evidence(instance, cursor);
                    CREATE INDEX evidence_expiry ON evidence(expires);
                    CREATE TABLE captures (
                        instance TEXT PRIMARY KEY, revoked INTEGER NOT NULL DEFAULT 0,
                        expired_records INTEGER NOT NULL DEFAULT 0
                    );",
                )
                .map_err(storage)?;
                tx.pragma_update(None, "application_id", EVIDENCE_APPLICATION_ID)
                    .map_err(storage)?;
                tx.pragma_update(None, "user_version", SCHEMA)
                    .map_err(storage)?;
            }
            (EVIDENCE_APPLICATION_ID, SCHEMA) => {}
            _ => return Err(EvidenceError::Schema),
        }
        tx.commit().map_err(storage)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(storage)?;
        conn.pragma_update(None, "secure_delete", "ON")
            .map_err(storage)?;
        let page_bytes: usize = conn
            .pragma_query_value(None, "page_size", |r| r.get(0))
            .map_err(storage)?;
        let max_pages = limits.database_bytes / page_bytes;
        let cap: usize = conn
            .query_row(&format!("PRAGMA max_page_count = {max_pages}"), [], |r| {
                r.get(0)
            })
            .map_err(storage)?;
        if cap > max_pages {
            return Err(EvidenceError::InvalidLimits);
        }
        let mut database = Self { conn, limits };
        database.purge()?;
        Ok(database)
    }

    fn process(&mut self, command: Command, drops: &AtomicU64) {
        match command {
            Command::Record {
                instance,
                id,
                record,
            } => {
                if self.insert(instance, id, &record).is_err() {
                    drops.fetch_add(1, Ordering::Relaxed);
                }
            }
            Command::Read {
                instance,
                cursor,
                limit,
                purge,
                reply,
            } => {
                let maintenance = if purge { self.purge() } else { Ok(()) };
                let _ = reply.send(maintenance.and_then(|()| self.read(instance, cursor, limit)));
            }
            Command::Contains {
                instance,
                payloads,
                reply,
            } => {
                let _ = reply.send(self.conn.query_row(
                    "SELECT EXISTS (SELECT 1 FROM evidence WHERE instance = ?1 AND expires > ?2 AND payload = ?3)
                      AND EXISTS (SELECT 1 FROM evidence WHERE instance = ?1 AND expires > ?2 AND payload = ?4)",
                    params![instance.0.to_string(), now(), payloads[0], payloads[1]],
                    |row| row.get(0),
                ).map_err(storage));
            }
            Command::Revoke { instance, reply } => {
                let _ = reply.send(self.revoke(instance));
            }
        }
    }

    fn insert(
        &mut self,
        instance: RunId,
        id: Uuid,
        record: &EvidenceRecord,
    ) -> Result<(), EvidenceError> {
        let expires = (record.at_ms + self.limits.retention.as_millis() as u64) as i64;
        if expires <= now() {
            return Ok(());
        }
        let payload = encode(record, self.limits.record_bytes)?;
        let instance = instance.0.to_string();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute(
            "INSERT OR IGNORE INTO captures (instance) VALUES (?1)",
            [&instance],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT OR IGNORE INTO evidence (instance, id, expires, payload)
             SELECT ?1, ?2, ?3, ?4 WHERE EXISTS
                (SELECT 1 FROM captures WHERE instance = ?1 AND revoked = 0)",
            params![instance, id.to_string(), expires, payload],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }

    fn read(
        &mut self,
        instance: RunId,
        cursor: i64,
        limit: usize,
    ) -> Result<EvidencePage, EvidenceError> {
        // Use one snapshot for revocation and the page, including across store handles.
        let tx = self.conn.transaction().map_err(storage)?;
        let (revoked, expired_records) = tx
            .query_row(
                "SELECT revoked, expired_records FROM captures WHERE instance = ?1",
                [instance.0.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage)?
            .unwrap_or_default();
        let mut query = tx.prepare(
                "SELECT cursor, expires, CASE WHEN expires > ?3 AND length(payload) <= ?5 THEN payload END
             FROM evidence INDEXED BY evidence_instance WHERE instance = ?1 AND cursor > ?2
             ORDER BY cursor LIMIT ?4",
            )
            .map_err(storage)?;
        let cutoff = now();
        let mut rows = query
            .query(params![
                instance.0.to_string(),
                cursor,
                cutoff,
                (limit + 1) as i64,
                self.limits.record_bytes as i64
            ])
            .map_err(storage)?;
        let mut page = EvidencePage {
            records: Vec::new(),
            next_cursor: None,
            invalid_records: 0,
            expired_records,
            revoked,
            observed_drops: 0,
        };
        let mut last = cursor;
        let mut scanned = 0;
        while let Some(row) = rows.next().map_err(storage)? {
            if scanned == limit {
                page.next_cursor = Some(last);
                break;
            }
            scanned += 1;
            last = row.get(0).map_err(storage)?;
            if row.get::<_, i64>(1).map_err(storage)? <= cutoff {
                page.expired_records = page.expired_records.saturating_add(1);
                continue;
            }
            match row
                .get_ref(2)
                .ok()
                .and_then(|value| value.as_blob().ok())
                .and_then(|payload| serde_json::from_slice::<EvidenceRecord>(payload).ok())
            {
                Some(record) if record.session_instance == instance => page.records.push(record),
                _ => page.invalid_records += 1,
            }
        }
        Ok(page)
    }

    fn purge(&mut self) -> Result<(), EvidenceError> {
        let now = now();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute(
            "INSERT INTO captures (instance, expired_records)
             SELECT instance, count(*) FROM evidence WHERE expires <= ?1 GROUP BY instance
             ON CONFLICT(instance) DO UPDATE SET expired_records = captures.expired_records + excluded.expired_records",
            [now],
        ).map_err(storage)?;
        tx.execute("DELETE FROM evidence WHERE expires <= ?1", [now])
            .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    fn revoke(&mut self, instance: RunId) -> Result<(), EvidenceError> {
        let instance = instance.0.to_string();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute("DELETE FROM evidence WHERE instance = ?1", [&instance])
            .map_err(storage)?;
        tx.execute(
            "INSERT INTO captures (instance, revoked) VALUES (?1, 1)
                    ON CONFLICT(instance) DO UPDATE SET revoked = 1, expired_records = 0",
            [&instance],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }
}

#[cfg(unix)]
fn prepare_private_file(path: &Path) -> Result<(), EvidenceError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    // allow-blocking-io: Database::open runs on the blocking pool.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|_| EvidenceError::Storage)?;
    let metadata = file.metadata().map_err(|_| EvidenceError::Storage)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(EvidenceError::UnsafeFilePermissions);
    }
    Ok(())
}

fn now() -> i64 {
    super::super::log::now_ms() as i64
}

fn storage(error: rusqlite::Error) -> EvidenceError {
    if error.sqlite_error_code() == Some(ErrorCode::DiskFull) {
        EvidenceError::Full
    } else {
        EvidenceError::Storage
    }
}

pub(super) fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, EvidenceError> {
    struct Bounded(Vec<u8>, usize);

    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.1.saturating_sub(self.0.len()) {
                return Err(io::Error::other("evidence size limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut out = Bounded(Vec::new(), limit);
    serde_json::to_writer(&mut out, value).map_err(|_| EvidenceError::InvalidRecord)?;
    Ok(out.0)
}

#[cfg(test)]
#[path = "store_test.rs"]
mod tests;
