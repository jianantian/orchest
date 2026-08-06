use orchest::atomic::{complete as atomic_complete, CompletionRequest};
use orchest::model::{ProviderRuntimeConfig, ResponseFormat};
use orchest::run::RetryPolicy;
use orchest_provider::create_adapter_from_config;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

#[pyfunction]
#[pyo3(signature = (model, user, system=None, api_key=None, api_key_env=None, api_url=None, json_mode=false, retry=false, request_options=None))]
#[allow(clippy::too_many_arguments)] // justified: pyo3 function maps Python kwargs 1:1
pub fn complete(
    py: Python<'_>,
    model: String,
    user: String,
    system: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    json_mode: bool,
    retry: bool,
    request_options: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    let mut options = if let Some(value) = request_options {
        let json: String = value
            .py()
            .import("json")?
            .call_method1("dumps", (&value,))?
            .extract()?;
        super::parse_request_options_value(Some(
            serde_json::from_str::<Value>(&json)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?,
        ))
        .map_err(PyRuntimeError::new_err)?
    } else {
        Default::default()
    };
    if json_mode {
        options.response_format = ResponseFormat::JsonObject;
    }
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model,
        api_key,
        api_key_env,
        api_url,
        max_tokens: options.max_tokens,
    })
    .map_err(super::error::model_error)?;
    let request = CompletionRequest {
        system,
        user,
        options,
        retry_policy: retry.then(RetryPolicy::recommended),
    };
    py.detach(|| {
        tokio::runtime::Runtime::new()
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?
            .block_on(atomic_complete(adapter.as_ref(), request))
            .map_err(super::error::model_error)
    })
}
