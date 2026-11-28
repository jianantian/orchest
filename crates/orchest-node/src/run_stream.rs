use std::sync::Arc;

use napi_derive::napi;
use orchest::bindings::run::{BindingRunError, BindingRunStream};
use orchest::bindings::run_event_to_wire_value;
use orchest::bindings::session::BindingSessionOptions;
use serde_json::Value;

pub fn run_error(error: BindingRunError) -> napi::Error {
    napi::Error::from_reason(format!("__ORCHEST_RUN_ERROR__:{}:{error}", error.code()))
}

#[napi(object)]
pub struct SessionOptions {
    pub sqlite_path: Option<String>,
    pub session_id: Option<String>,
}

impl SessionOptions {
    pub fn into_binding(self) -> Result<BindingSessionOptions, BindingRunError> {
        BindingSessionOptions::new(self.sqlite_path, self.session_id)
    }
}

#[napi]
pub struct NativeRun {
    pub(crate) stream: Arc<BindingRunStream>,
}

impl Drop for NativeRun {
    fn drop(&mut self) {
        self.stream.stop_reading();
    }
}

#[napi]
impl NativeRun {
    #[napi]
    pub async fn next_event(&self) -> napi::Result<Option<Value>> {
        let Some(event) = self.stream.next().await else {
            return Ok(None);
        };
        let converted = run_event_to_wire_value(&event);
        if converted.is_err() {
            self.stream.close().await;
        }
        converted
            .map(Some)
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    #[napi]
    pub async fn wait(&self) -> napi::Result<Value> {
        let event = self.stream.wait().await.map_err(run_error)?;
        run_event_to_wire_value(&event).map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    #[napi]
    pub async fn close(&self) {
        self.stream.close().await;
    }

    #[napi]
    pub async fn respond(&self, request_id: String, payload: Value) -> napi::Result<()> {
        self.stream
            .respond(request_id, payload)
            .await
            .map_err(run_error)
    }

    #[napi]
    pub fn capabilities(&self) -> napi::Result<Value> {
        serde_json::to_value(self.stream.capabilities())
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    #[napi]
    pub fn cancel(&self, reason: Option<String>, message: Option<String>) -> napi::Result<bool> {
        let cancellation =
            orchest::bindings::parse_binding_cancellation(reason.as_deref(), message)
                .map_err(napi::Error::from_reason)?;
        Ok(self.stream.cancel(cancellation))
    }
}
