use std::sync::{Arc, Mutex};

use bytes::Bytes;
use orchest_protocol::{Language, SessionInput, StreamingTranscribeRequest};
use orchest_provider::{ProviderConfig, Registry};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::sync::Notify;

const DEFAULT_STREAMING_ASR: &str = "aliyun/qwen-audio-3.0-asr-flash-streaming";

struct Completion {
    result: Mutex<Option<Result<(), String>>>,
    notify: Notify,
}

#[pyclass(name = "_NativeAsrStream")]
pub struct NativeAsrStream {
    input: Arc<Mutex<Option<tokio::sync::mpsc::Sender<SessionInput>>>>,
    completion: Arc<Completion>,
}

#[pymethods]
impl NativeAsrStream {
    fn send_audio<'py>(&self, py: Python<'py>, audio: Vec<u8>) -> PyResult<Bound<'py, PyAny>> {
        if audio.is_empty() {
            return Err(PyRuntimeError::new_err("audio chunk cannot be empty"));
        }
        let input = Arc::clone(&self.input);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let sender = input
                .lock()
                .map_err(|_| PyRuntimeError::new_err("ASR session lock poisoned"))?
                .clone()
                .ok_or_else(|| PyRuntimeError::new_err("ASR session is finishing or closed"))?;
            sender
                .send(SessionInput::Audio(Bytes::from(audio)))
                .await
                .map_err(|_| PyRuntimeError::new_err("ASR session input is closed"))
        })
    }

    fn finish(&self) -> PyResult<()> {
        self.input
            .lock()
            .map_err(|_| PyRuntimeError::new_err("ASR session lock poisoned"))?
            .take();
        Ok(())
    }

    fn wait<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let completion = Arc::clone(&self.completion);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                let notified = completion.notify.notified();
                if let Some(result) = completion
                    .result
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("ASR completion lock poisoned"))?
                    .clone()
                {
                    return result.map_err(PyRuntimeError::new_err);
                }
                notified.await;
            }
        })
    }
}

impl Drop for NativeAsrStream {
    fn drop(&mut self) {
        if let Ok(mut input) = self.input.lock() {
            input.take();
        }
    }
}

#[pyfunction(name = "_start_asr_stream")]
#[pyo3(signature = (format, sample_rate, on_event, language=None, provider=None, api_key=None, api_key_env=None, api_url=None, context=None, options=None))]
#[allow(clippy::too_many_arguments)] // justified: pyo3 function maps Python kwargs 1:1
pub fn start_asr_stream<'py>(
    py: Python<'py>,
    format: &str,
    sample_rate: u32,
    on_event: Py<PyAny>,
    language: Option<String>,
    provider: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    context: Option<Bound<'_, PyAny>>,
    options: Option<Bound<'_, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let format = super::asr::parse_audio_format(format)?;
    let mut options = super::asr::py_dict_value(options)?;
    if options.is_null() {
        options = serde_json::json!({});
    }
    if !options.is_object() {
        return Err(PyRuntimeError::new_err("options must be an object"));
    }
    options["sample_rate"] = serde_json::json!(sample_rate);
    if let Some(context) = context {
        let json: String = py
            .import("json")?
            .call_method1("dumps", (context,))?
            .extract()?;
        let context: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        validate_context(&context)?;
        options["context"] = context;
    }
    let id = provider.unwrap_or_else(|| DEFAULT_STREAMING_ASR.to_string());
    let key = super::asr::resolve_key(
        api_key,
        api_key_env.as_deref(),
        super::asr::default_key_env(&id),
    )?;

    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let registry = Registry::with_builtin();
        let entry = registry
            .asr()
            .id(&id)
            .select()
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let (provider_name, model) = id
            .split_once('/')
            .ok_or_else(|| PyRuntimeError::new_err("provider must be provider/model"))?;
        let handle = entry
            .instantiate(&ProviderConfig {
                provider: provider_name.into(),
                model: model.into(),
                api_key: Some(key),
                api_url,
                max_tokens: None,
                options: serde_json::Value::Null,
            })
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let realtime = handle
            .start_stream(StreamingTranscribeRequest {
                format,
                language: language.map(Language),
                options,
            })
            .await
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let input = Arc::new(Mutex::new(Some(realtime.input)));
        let completion = Arc::new(Completion {
            result: Mutex::new(None),
            notify: Notify::new(),
        });
        let completion_task = Arc::clone(&completion);
        tokio::spawn(async move {
            let mut events = realtime.events;
            let mut result = Ok(());
            while let Some(event) = events.next().await {
                let fatal_error = match &event {
                    orchest_protocol::StreamEvent::Error {
                        error, fatal: true, ..
                    } => Some(error.to_string()),
                    _ => None,
                };
                let event_json = match serde_json::to_string(&event) {
                    Ok(value) => value,
                    Err(error) => {
                        result = Err(error.to_string());
                        break;
                    }
                };
                let callback_result = Python::attach(|py| -> PyResult<()> {
                    let value = py.import("json")?.call_method1("loads", (event_json,))?;
                    on_event.call1(py, (value,))?;
                    Ok(())
                });
                if let Err(error) = callback_result {
                    result = Err(error.to_string());
                    break;
                }
                if let Some(error) = fatal_error {
                    result = Err(error);
                    break;
                }
            }
            if let Ok(mut slot) = completion_task.result.lock() {
                *slot = Some(result);
            }
            completion_task.notify.notify_waiters();
        });
        Ok(NativeAsrStream { input, completion })
    })
}

fn validate_context(context: &serde_json::Value) -> PyResult<()> {
    let valid = context.as_array().is_some_and(|messages| {
        messages.iter().all(|message| {
            let role = message.get("role").and_then(serde_json::Value::as_str);
            let text = message.get("text").and_then(serde_json::Value::as_str);
            matches!(role, Some("user" | "assistant")) && text.is_some()
        })
    });
    if valid {
        Ok(())
    } else {
        Err(PyRuntimeError::new_err(
            "context must be an array of {'role': 'user' | 'assistant', 'text': str} objects",
        ))
    }
}
