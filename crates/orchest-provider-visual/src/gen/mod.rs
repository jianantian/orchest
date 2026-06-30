//! Signed / polled image+video generation dialects (the gen weight tier). These
//! implement the spine [`orchest_protocol::GenTask`] (submit → poll → fetch) over
//! `reqwest`, abstracted from the old concrete `ImageGateway`. Registered through
//! the wall via [`crate::gen_entries`].
//!
//! Some upstream APIs are **synchronous** (the result returns inline from the
//! submit call, with no job id to poll). [`SyncGenCache`] adapts those onto the
//! submit → poll → fetch contract without re-generating on `fetch`.

use std::collections::HashMap;
use std::sync::Mutex;

use orchest_protocol::{ErrorCode, GenHandle, GenResult, GenStatus, ProtocolError};

pub mod aliyun;
pub mod crazyrouter;
pub mod renderful;

/// Adapts a **synchronous** gen API (the asset result is returned inline from the
/// submit call) onto the [`GenTask`](orchest_protocol::GenTask) submit → poll →
/// fetch lifecycle: `submit` runs the generation and [`store`](Self::store)s the
/// [`GenResult`] under a fresh id; `poll` reports `Done` while it is cached;
/// `fetch` returns the cached result — so no second generation is issued.
#[derive(Default)]
pub struct SyncGenCache {
    results: Mutex<HashMap<String, GenResult>>,
}

impl SyncGenCache {
    /// Store a completed result and return the handle that addresses it.
    pub fn store(&self, provider: &str, result: GenResult) -> GenHandle {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.lock().insert(id.clone(), result);
        GenHandle {
            id,
            provider: Some(provider.to_string()),
        }
    }

    /// `Done` while the result is cached; `Failed` if the handle is unknown.
    pub fn status(&self, handle: &GenHandle) -> GenStatus {
        if self.lock().contains_key(&handle.id) {
            GenStatus::Done
        } else {
            GenStatus::Failed
        }
    }

    /// Return the cached result for `handle`, or an error if it is unknown.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        self.lock().get(&handle.id).cloned().ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::InvalidRequest,
                "unknown gen handle (already fetched or never submitted)",
            )
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, GenResult>> {
        self.results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sync_cache_round_trips_submit_poll_fetch() {
        let cache = SyncGenCache::default();
        let result = GenResult {
            assets: Vec::new(),
            diagnostic_metadata: json!({"k": "v"}),
        };
        let handle = cache.store("crazyrouter", result.clone());
        assert_eq!(handle.provider.as_deref(), Some("crazyrouter"));
        assert_eq!(cache.status(&handle), GenStatus::Done);
        assert_eq!(cache.fetch(&handle).unwrap(), result);

        let unknown = GenHandle {
            id: "nope".to_string(),
            provider: None,
        };
        assert_eq!(cache.status(&unknown), GenStatus::Failed);
        assert!(cache.fetch(&unknown).is_err());
    }
}
