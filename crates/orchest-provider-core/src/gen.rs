//! Shared gen-task helpers.
//!
//! [`SyncGenCache`] adapts a **synchronous** generation API — one whose asset
//! result is returned inline from the submit call, with no job id to poll — onto
//! the spine [`GenTask`](orchest_protocol::GenTask) submit → poll → fetch
//! lifecycle, without re-generating on `fetch`. Both the REST tier
//! (`orchest-provider-http`, e.g. minimax music) and the gen tier
//! (`orchest-provider-visual`, e.g. crazyrouter / volcengine Ark) use it.

use std::collections::HashMap;
use std::sync::Mutex;

use orchest_protocol::{ErrorCode, GenHandle, GenResult, GenStatus, ProtocolError};

/// A per-provider cache that presents a synchronous gen result as a
/// submit → poll → fetch job: [`store`](Self::store) the completed [`GenResult`]
/// under a fresh id (returning the handle), [`status`](Self::status) reports
/// `Done` while cached, and [`fetch`](Self::fetch) returns it.
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
            diagnostic_metadata: json!({ "k": "v" }),
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
