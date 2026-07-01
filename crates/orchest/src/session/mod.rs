//! Session persistence: store, snapshot, and auto-save hook.

pub mod persistence_hook;
pub mod snapshot;
#[cfg(feature = "sqlite-session")]
pub mod sqlite;
pub mod store;

pub use persistence_hook::SessionPersistenceHook;
pub use snapshot::SessionSnapshot;
#[cfg(feature = "sqlite-session")]
pub use sqlite::SqliteSessionStore;
pub use store::{InMemorySessionStore, SessionError, SessionStore};
