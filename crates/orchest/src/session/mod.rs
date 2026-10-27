//! Session persistence: the append-only event log, its projection, and the
//! stores that hold it.

pub mod log;
#[cfg(feature = "sqlite-session")]
pub mod sqlite;
pub mod store;

pub use log::{SessionEvent, SessionEventKind, SessionLog, SessionProjection};
#[cfg(feature = "sqlite-session")]
pub use sqlite::SqliteSessionStore;
pub use store::{CheckpointPolicy, InMemorySessionStore, SessionError, SessionMeta, SessionStore};
