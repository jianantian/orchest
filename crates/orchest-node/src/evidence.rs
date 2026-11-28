use napi_derive::napi;
use orchest::bindings::session::BindingSessionOptions;
use orchest::session::evidence::{AttemptReader, EvidenceError, EvidenceLimits, EvidenceStore};

use crate::run_stream::run_error;

#[napi]
pub struct NativeEvidenceStore(pub(crate) EvidenceStore);

#[napi(js_name = "_openEvidenceStore")]
pub async fn open_evidence_store(path: String) -> napi::Result<NativeEvidenceStore> {
    EvidenceStore::open(path, EvidenceLimits::default())
        .await
        .map(NativeEvidenceStore)
        .map_err(|error| run_error(error.into()))
}

#[napi]
impl NativeEvidenceStore {
    #[napi]
    pub async fn authorize(
        &self,
        sqlite_path: String,
        session_id: String,
    ) -> napi::Result<NativeAttemptReader> {
        BindingSessionOptions::new(Some(sqlite_path), Some(session_id))
            .map_err(run_error)?
            .authorize_evidence(self.0.clone())
            .await
            .map(NativeAttemptReader)
            .map_err(run_error)
    }
}

#[napi]
pub struct NativeAttemptReader(AttemptReader);

#[napi]
impl NativeAttemptReader {
    #[napi]
    pub async fn read(
        &self,
        cursor: Option<String>,
        limit: f64,
    ) -> napi::Result<serde_json::Value> {
        if !limit.is_finite() || limit.fract() != 0.0 || !(1.0..=64.0).contains(&limit) {
            return Err(run_error(EvidenceError::InvalidPageSize.into()));
        }
        let page = self
            .0
            .read(cursor.as_deref(), limit as usize)
            .await
            .map_err(|error| run_error(error.into()))?;
        serde_json::to_value(page).map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    #[napi]
    pub async fn revoke(&self) -> napi::Result<()> {
        self.0
            .revoke()
            .await
            .map_err(|error| run_error(error.into()))
    }
}
