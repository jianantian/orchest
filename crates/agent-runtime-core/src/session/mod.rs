//! Session persistence: store, snapshot, and auto-save hook.

pub mod persistence_hook;
pub mod snapshot;
pub mod store;
#[cfg(feature = "sqlite-session")]
pub mod sqlite;

pub use persistence_hook::SessionPersistenceHook;
pub use snapshot::SessionSnapshot;
pub use store::{InMemorySessionStore, SessionError, SessionStore};
#[cfg(feature = "sqlite-session")]
pub use sqlite::SqliteSessionStore;
