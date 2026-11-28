use orchest::bindings::session::BindingSessionOptions;
use orchest::session::evidence::{AttemptReader, EvidenceError, EvidenceLimits, EvidenceStore};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBool;

use crate::run_stream::run_error;

#[pyclass(name = "EvidenceStore", frozen)]
pub struct PyEvidenceStore(pub EvidenceStore);

#[pymethods]
impl PyEvidenceStore {
    #[staticmethod]
    fn open(py: Python<'_>, path: String) -> PyResult<Bound<'_, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            EvidenceStore::open(path, EvidenceLimits::default())
                .await
                .map(Self)
                .map_err(|error| run_error(error.into()))
        })
    }

    fn authorize<'py>(
        &self,
        py: Python<'py>,
        sqlite_path: String,
        session_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let options =
            BindingSessionOptions::new(Some(sqlite_path), Some(session_id)).map_err(run_error)?;
        let store = self.0.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            options
                .authorize_evidence(store)
                .await
                .map(PyAttemptReader)
                .map_err(run_error)
        })
    }
}

#[pyclass(name = "AttemptReader", frozen)]
pub struct PyAttemptReader(AttemptReader);

#[pymethods]
impl PyAttemptReader {
    #[pyo3(signature = (cursor=None, *, limit=None))]
    fn read<'py>(
        &self,
        py: Python<'py>,
        cursor: Option<&Bound<'py, PyAny>>,
        limit: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let cursor = cursor
            .map(|value| value.extract::<String>())
            .transpose()
            .map_err(|_| run_error(EvidenceError::InvalidCursor.into()))?;
        if limit.is_some_and(|value| value.is_instance_of::<PyBool>()) {
            return Err(run_error(EvidenceError::InvalidPageSize.into()));
        }
        let limit = limit
            .map(|value| value.extract::<usize>())
            .transpose()
            .map_err(|_| run_error(EvidenceError::InvalidPageSize.into()))?
            .unwrap_or(64);
        let reader = self.0.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = reader
                .read(cursor.as_deref(), limit)
                .await
                .map_err(|error| run_error(error.into()))?;
            let json = serde_json::to_string(&page)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            Python::attach(|py| Ok(py.import("json")?.call_method1("loads", (json,))?.unbind()))
        })
    }

    fn revoke<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let reader = self.0.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            reader
                .revoke()
                .await
                .map_err(|error| run_error(error.into()))
        })
    }
}
