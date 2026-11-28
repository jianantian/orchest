use std::sync::Arc;

use orchest::bindings::run::{BindingRunError, BindingRunStream};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

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

    #[pyo3(signature = (reason=None, message=None))]
    fn cancel(&self, reason: Option<String>, message: Option<String>) -> PyResult<bool> {
        let cancellation =
            orchest::bindings::parse_binding_cancellation(reason.as_deref(), message)
                .map_err(PyValueError::new_err)?;
        Ok(self.0.cancel(cancellation))
    }
}
