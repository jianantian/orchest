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
use serde_json::Value;

/// A per-provider cache that presents a synchronous gen result as a
/// submit → poll → fetch job: [`store`](Self::store) the completed [`GenResult`]
/// under a fresh id (returning the handle), [`status`](Self::status) reports
/// `Done` while cached, and [`fetch`](Self::fetch) returns it **once**, consuming
/// the entry. Fetch is terminal in the submit → poll → fetch lifecycle, so
/// consuming on fetch bounds the cache to in-flight (submitted-but-unfetched)
/// results rather than retaining every asset for the process lifetime.
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

    /// Return the cached result for `handle`, **consuming** it (fetch-once), or an
    /// error if it is unknown or already fetched. Consuming here is what bounds the
    /// cache — nothing else evicts.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        self.lock().remove(&handle.id).ok_or_else(|| {
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

/// Warn once per [`GenRequest::params`](orchest_protocol::GenRequest) key that
/// a gen provider's submit path does **not** consume (v0.15, issue 001).
///
/// `params` is the dialect escape hatch: a misspelled or unsupported key used
/// to be dropped silently, so every gen provider that cherry-picks keys calls
/// this at the top of its submit-body builder with its full consumed set
/// (explicitly handled keys + passthrough whitelist + the wire names of the
/// typed [`MusicParams`](orchest_protocol::MusicParams) fields it maps).
/// Providers that forward `params` verbatim still pass an unknown key on to
/// the API — the warning names it as *not known to the SDK*, not as certainly
/// inert.
///
/// Cardinality discipline: the key name travels as the event's `param_key`
/// field — never in a metric label — so arbitrary user key spellings cannot
/// explode label cardinality. Non-object `params` (e.g. `Value::Null`) warns
/// on nothing.
///
/// Wired into every gen submit path that cherry-picks `params` keys (keep in
/// sync when adding a provider): **orchest-provider-http** — `gen/suno.rs`,
/// `gen/mureka.rs`, `gen/minimax_music.rs`, `gen/aliyun_music.rs`;
/// **orchest-provider-visual** — `gen/aliyun.rs`, `gen/volcengine.rs`,
/// `gen/volcengine_video.rs`, `gen/crazyrouter.rs`, `gen/renderful.rs`.
pub fn warn_unconsumed_params(provider: &str, consumed: &[&str], params: &Value) {
    let Some(object) = params.as_object() else {
        return;
    };
    for key in object.keys() {
        if !consumed.contains(&key.as_str()) {
            tracing::warn!(
                provider,
                param_key = key.as_str(),
                "gen params key is not a known knob for this provider (possible typo); providers that pass params through verbatim still forward it"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex as StdMutex};

    /// A `MakeWriter` over a shared buffer so tests can assert on the tracing
    /// output the warning helper emits.
    #[derive(Clone, Default)]
    struct SharedBuffer(Arc<StdMutex<Vec<u8>>>);

    impl std::io::Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl tracing_subscriber::fmt::MakeWriter<'_> for SharedBuffer {
        type Writer = SharedBuffer;

        fn make_writer(&self) -> Self::Writer {
            self.clone()
        }
    }

    /// Run `f` with a fmt subscriber writing into a buffer; return the logs.
    fn captured_logs(f: impl FnOnce()) -> String {
        let buffer = SharedBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buffer.clone())
            .with_ansi(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        let bytes = buffer
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        String::from_utf8(bytes).expect("tracing output is utf8")
    }

    #[test]
    fn warn_unconsumed_params_names_unknown_keys_only() {
        let logs = captured_logs(|| {
            warn_unconsumed_params(
                "suno",
                &["style", "title"],
                &json!({"style": "lofi", "genre": "indie folk", "tempo": "slow"}),
            );
        });
        assert!(logs.contains("WARN"), "expected a WARN event: {logs}");
        assert!(
            logs.contains("suno"),
            "provider name in span fields: {logs}"
        );
        assert!(logs.contains("genre"), "unknown key named: {logs}");
        assert!(logs.contains("tempo"), "unknown key named: {logs}");
        assert!(
            !logs.contains("style"),
            "consumed key must not warn: {logs}"
        );
    }

    #[test]
    fn warn_unconsumed_params_ignores_non_object_params() {
        for params in [Value::Null, json!([]), json!("lofi")] {
            let logs = captured_logs(|| warn_unconsumed_params("suno", &[], &params));
            assert!(logs.is_empty(), "no warning for {params}: {logs}");
        }
    }

    #[test]
    fn warn_unconsumed_params_empty_consumed_list_warns_on_everything() {
        let logs = captured_logs(|| {
            warn_unconsumed_params("mureka", &[], &json!({"lyrics": "la"}));
        });
        assert!(logs.contains("lyrics"), "{logs}");
    }

    #[test]
    fn sync_cache_round_trips_submit_poll_fetch() {
        let cache = SyncGenCache::default();
        let result = GenResult {
            assets: Vec::new(),
            diagnostic_metadata: json!({ "k": "v" }),
            timed_text: None,
            duration_secs: None,
        };
        let handle = cache.store("crazyrouter", result.clone());
        assert_eq!(handle.provider.as_deref(), Some("crazyrouter"));
        assert_eq!(cache.status(&handle), GenStatus::Done);
        assert_eq!(cache.fetch(&handle).unwrap(), result);

        // fetch consumes: the entry is gone afterwards, so the cache does not
        // retain results for the process lifetime. A second fetch/status fails.
        assert_eq!(cache.status(&handle), GenStatus::Failed);
        assert!(cache.fetch(&handle).is_err());

        let unknown = GenHandle {
            id: "nope".to_string(),
            provider: None,
        };
        assert_eq!(cache.status(&unknown), GenStatus::Failed);
        assert!(cache.fetch(&unknown).is_err());
    }
}
