use orchest_protocol::{ModelError, ProtocolError};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ProviderErrorData {
    message: String,
    code: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    status: Option<u16>,
    retry_after_secs: Option<u64>,
    upstream: Option<Value>,
    diagnostic_metadata: Option<Value>,
}

impl ProviderErrorData {
    pub fn into_pyerr(self) -> PyErr {
        Python::attach(|py| self.build_pyerr(py)).unwrap_or_else(|error| {
            PyRuntimeError::new_err(format!("failed to construct provider exception: {error}"))
        })
    }

    fn build_pyerr(self, py: Python<'_>) -> PyResult<PyErr> {
        let instance = py
            .import("orchest.exceptions")?
            .getattr("ModelError")?
            .call1((self.message, self.code))?;
        instance.setattr("provider", self.provider)?;
        instance.setattr("model", self.model)?;
        instance.setattr("status", self.status)?;
        instance.setattr("retry_after_secs", self.retry_after_secs)?;
        instance.setattr("upstream", json_to_python(py, self.upstream)?)?;
        instance.setattr(
            "diagnostic_metadata",
            json_to_python(py, self.diagnostic_metadata)?,
        )?;
        Ok(PyErr::from_value(instance))
    }
}

impl From<ModelError> for ProviderErrorData {
    fn from(error: ModelError) -> Self {
        Self {
            message: error.message,
            code: error.code,
            provider: error.provider,
            model: None,
            status: error.status,
            retry_after_secs: error.retry_after_secs,
            upstream: error
                .upstream
                .and_then(|value| serde_json::to_value(value).ok()),
            diagnostic_metadata: None,
        }
    }
}

impl From<ProtocolError> for ProviderErrorData {
    fn from(error: ProtocolError) -> Self {
        Self {
            message: error.message,
            code: serde_json::to_value(error.code)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned)),
            provider: error.provider,
            model: error.model,
            status: error.status,
            retry_after_secs: error.retry_after_secs,
            upstream: error
                .upstream
                .and_then(|value| serde_json::to_value(value).ok()),
            diagnostic_metadata: (!error.diagnostic_metadata.is_null())
                .then_some(error.diagnostic_metadata),
        }
    }
}

pub fn model_error(error: ModelError) -> PyErr {
    ProviderErrorData::from(error).into_pyerr()
}

pub fn protocol_error(error: ProtocolError) -> PyErr {
    ProviderErrorData::from(error).into_pyerr()
}

fn json_to_python<'py>(py: Python<'py>, value: Option<Value>) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Some(value) => py
            .import("json")?
            .call_method1("loads", (value.to_string(),)),
        None => Ok(py.None().into_bound(py)),
    }
}
