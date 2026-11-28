use std::sync::Arc;

use orchest::bindings::run::{BindingRunError, BindingRunStream};
use orchest::bindings::session::BindingSessionOptions;
use orchest::session::SessionError;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::runtime_event_to_dict;

pub fn run_error(error: BindingRunError) -> PyErr {
    Python::attach(|py| -> PyResult<PyErr> {
        let instance = py
            .import("orchest.exceptions")?
            .getattr("AgentError")?
            .call1((error.to_string(), error.code()))?;
        Ok(PyErr::from_value(instance))
    })
    .unwrap_or_else(|_| PyRuntimeError::new_err(error.to_string()))
}

pub fn session_options(session: &Bound<'_, PyDict>) -> PyResult<BindingSessionOptions> {
    let string = |key| {
        session
            .get_item(key)?
            .map(|value| value.extract::<String>())
            .transpose()
            .map_err(|_| run_error(SessionError::NotConfigured.into()))
    };
    BindingSessionOptions::new(string("sqlite_path")?, string("session_id")?).map_err(run_error)
}

pub fn wrap_run(stream: BindingRunStream) -> PyResult<Py<PyAny>> {
    Python::attach(|py| {
        let native = Py::new(py, NativeRun(Arc::new(stream)))?;
        let value = py.import("orchest")?.getattr("Run")?.call1((native,))?;
        Ok(value.unbind())
    })
}

#[pyclass(name = "_NativeRun")]
pub struct NativeRun(pub Arc<BindingRunStream>);

impl Drop for NativeRun {
    fn drop(&mut self) {
        self.0.stop_reading();
    }
}

#[pymethods]
impl NativeRun {
    fn next_event<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = Arc::clone(&self.0);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let Some(event) = stream.next().await else {
                return Ok(None);
            };
            let converted = Python::attach(|py| runtime_event_to_dict(py, &event));
            if converted.is_err() {
                stream.close().await;
            }
            converted.map(Some)
        })
    }

    fn wait<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = Arc::clone(&self.0);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let event = stream.wait().await.map_err(run_error)?;
            Python::attach(|py| runtime_event_to_dict(py, &event))
        })
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = Arc::clone(&self.0);
        stream.stop_reading();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            stream.close().await;
            Ok(())
        })
    }

    fn respond<'py>(
        &self,
        py: Python<'py>,
        request_id: String,
        payload: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let json = py
            .import("json")?
            .call_method1("dumps", (payload,))
            .and_then(|value| value.extract::<String>())
            .map_err(|error| run_error(BindingRunError::InvalidResponse(error.to_string())))?;
        let payload = serde_json::from_str(&json)
            .map_err(|error| run_error(BindingRunError::InvalidResponse(error.to_string())))?;
        let stream = Arc::clone(&self.0);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            stream.respond(request_id, payload).await.map_err(run_error)
        })
    }

    fn capabilities<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let json = serde_json::to_string(self.0.capabilities())
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        py.import("json")?.call_method1("loads", (json,))
    }

    #[pyo3(signature = (reason=None, message=None))]
    fn cancel(&self, reason: Option<String>, message: Option<String>) -> PyResult<bool> {
        let cancellation =
            orchest::bindings::parse_binding_cancellation(reason.as_deref(), message)
                .map_err(PyValueError::new_err)?;
        Ok(self.0.cancel(cancellation))
    }
}
