use std::sync::Arc;

use orchest::bindings::tts::TtsStream;
use orchest_protocol::{ErrorCode, ProtocolError};
use orchest_provider::tts::TtsOptions;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{json, Value};

use super::error::protocol_error;

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
fn parse_options(options: &str) -> Result<TtsOptions, ProtocolError> {
    let value = serde_json::from_str(options)
        .map_err(|error| ProtocolError::new(ErrorCode::InvalidRequest, error.to_string()))?;
    TtsOptions::parse(value)
}

fn as_dict<'py>(py: Python<'py>, value: Value) -> PyResult<Bound<'py, PyDict>> {
    let object = py
        .import("json")?
        .call_method1("loads", (value.to_string(),))?;
    Ok(object.cast_into::<PyDict>()?)
}

#[pyfunction(name = "_synthesize")]
pub fn synthesize<'py>(
    py: Python<'py>,
    text: String,
    options: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let options = parse_options(options).map_err(protocol_error)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let result = options.synthesize(text).await.map_err(protocol_error)?;
        Python::attach(|py| {
            let object = as_dict(
                py,
                json!({"format": result.format, "usage": result.voice_usage}),
            )?;
            object.set_item("audio", PyBytes::new(py, &result.audio))?;
            Ok(object.unbind())
        })
    })
}

#[pyclass(name = "_NativeTtsStream")]
pub struct NativeTtsStream {
    inner: Arc<TtsStream>,
}

#[pymethods]
impl NativeTtsStream {
    fn next_event<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let event = stream.next().await.map_err(protocol_error)?;
            Python::attach(|py| -> PyResult<Option<Py<PyDict>>> {
                let Some(event) = event else {
                    return Ok(None);
                };
                let object = as_dict(py, event.metadata)?;
                if let Some(audio) = event.audio {
                    object.set_item("audio", PyBytes::new(py, &audio))?;
                }
                Ok(Some(object.unbind()))
            })
        })
    }

    #[pyo3(signature = (action, text=None))]
    fn command<'py>(
        &self,
        py: Python<'py>,
        action: String,
        text: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            stream.command(&action, text).await.map_err(protocol_error)
        })
    }

    fn usage<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let usage = stream.usage().await;
            Python::attach(|py| -> PyResult<Option<Py<PyDict>>> {
                usage
                    .map(|usage| as_dict(py, json!(usage)).map(Bound::unbind))
                    .transpose()
            })
        })
    }

    fn abort<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            stream.abort().await;
            Ok(())
        })
    }
}

impl Drop for NativeTtsStream {
    fn drop(&mut self) {
        self.inner.cancel_pending();
    }
}

#[pyfunction(name = "_open_tts_stream")]
pub fn open_stream<'py>(
    py: Python<'py>,
    options: &str,
    text: Option<String>,
) -> PyResult<Bound<'py, PyAny>> {
    let options = parse_options(options).map_err(protocol_error)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let stream = match text {
            Some(text) => options
                .stream_synthesize(text)
                .await
                .map(|events| TtsStream::new(events, None)),
            None => options
                .start_duplex()
                .await
                .map(|handle| TtsStream::new(handle.events, Some(handle.input))),
        }
        .map_err(protocol_error)?;
        Ok(NativeTtsStream {
            inner: Arc::new(stream),
        })
    })
}
