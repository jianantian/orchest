use std::collections::BTreeMap;

use orchest_protocol::{DecisionQuestion, DecisionRequest, ErrorCode, ProtocolError};
use orchest_provider::{decide as provider_decide, DecisionConfig};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyfunction]
#[pyo3(signature = (*, model, state, questions, api_key=None, api_key_env=None, api_url=None, timeout_ms=None))]
#[allow(clippy::too_many_arguments)] // justified: maps the public Python keyword API 1:1
pub fn decide(
    py: Python<'_>,
    model: String,
    state: &Bound<'_, PyAny>,
    questions: &Bound<'_, PyDict>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    timeout_ms: Option<u64>,
) -> PyResult<Py<PyAny>> {
    let json = py.import("json")?;
    let state_json: String = json.call_method1("dumps", (state,))?.extract()?;
    let questions_json: String = json.call_method1("dumps", (questions,))?.extract()?;
    let request = DecisionRequest {
        state: serde_json::from_str(&state_json)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?,
        questions: serde_json::from_str::<BTreeMap<String, DecisionQuestion>>(&questions_json)
            .map_err(|error| {
                super::error::protocol_error(
                    ProtocolError::new(
                        ErrorCode::InvalidRequest,
                        format!("invalid decision questions: {error}"),
                    )
                    .with_model(model.clone()),
                )
            })?,
    };
    let config = DecisionConfig {
        model,
        api_key,
        api_key_env,
        api_url,
        timeout_ms,
    };

    let response = py.detach(|| {
        tokio::runtime::Runtime::new()
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?
            .block_on(provider_decide(&config, request))
            .map_err(super::error::protocol_error)
    })?;
    let value = serde_json::to_value(response)
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    json.call_method1("loads", (value.to_string(),))
        .map(Bound::unbind)
}
