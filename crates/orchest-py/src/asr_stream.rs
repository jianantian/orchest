use std::sync::{Arc, Mutex};

use bytes::Bytes;
use orchest::bindings::asr::{sdk_asr_event, select_asr_model, AsrMode};
use orchest_protocol::{
    ErrorCode, Language, ProtocolError, SessionInput, StreamEvent, StreamingTranscribeRequest,
};
use orchest_provider::{ProviderConfig, Registry};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::sync::Notify;

const DEFAULT_STREAMING_PROVIDER: &str = "aliyun";
const DEFAULT_STREAMING_MODEL: &str = "qwen-audio-3.0-asr-flash-streaming";

struct Completion {
    result: Mutex<Option<Result<(), StreamFailure>>>,
    notify: Notify,
}

#[derive(Debug, Clone)]
enum StreamFailure {
    Provider(super::error::ProviderErrorData),
    Runtime(String),
}

impl StreamFailure {
    fn into_pyerr(self) -> PyErr {
        match self {
            Self::Provider(error) => error.into_pyerr(),
            Self::Runtime(message) => PyRuntimeError::new_err(message),
        }
    }
}

#[pyclass(name = "_NativeAsrStream")]
pub struct NativeAsrStream {
    input: Arc<Mutex<Option<tokio::sync::mpsc::Sender<SessionInput>>>>,
    completion: Arc<Completion>,
    /// Whether the selected model honours `SessionInput::Finalize`.
    client_finalize: bool,
    provider: String,
    model: String,
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

    fn finalize<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if !self.client_finalize {
            return Err(super::error::protocol_error(
                ProtocolError::new(
                    ErrorCode::UnsupportedOption,
                    format!("{}/{} does not support finalize", self.provider, self.model),
                )
                .with_provider(self.provider.clone())
                .with_model(self.model.clone()),
            ));
        }
        let input = Arc::clone(&self.input);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let sender = input
                .lock()
                .map_err(|_| PyRuntimeError::new_err("ASR session lock poisoned"))?
                .clone()
                .ok_or_else(|| PyRuntimeError::new_err("ASR session is finishing or closed"))?;
            sender
                .send(SessionInput::Finalize)
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
                    return result.map_err(StreamFailure::into_pyerr);
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
#[pyo3(signature = (format, sample_rate, on_event, language=None, provider=None, model=None, api_key=None, api_key_env=None, api_url=None, context=None, options=None))]
#[allow(clippy::too_many_arguments)] // justified: pyo3 function maps Python kwargs 1:1
pub fn start_asr_stream<'py>(
    py: Python<'py>,
    format: &str,
    sample_rate: u32,
    on_event: Py<PyAny>,
    language: Option<String>,
    provider: Option<String>,
    model: Option<String>,
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
    let (provider_name, model) = match (provider, model) {
        (None, None) => (
            DEFAULT_STREAMING_PROVIDER.to_string(),
            Some(DEFAULT_STREAMING_MODEL.to_string()),
        ),
        (Some(provider), model) => (super::asr::validate_provider(provider)?, model),
        (None, Some(_)) => {
            return Err(PyRuntimeError::new_err("model requires provider"));
        }
    };
    let key = super::asr::resolve_key(
        api_key,
        api_key_env.as_deref(),
        super::asr::default_key_env(&provider_name),
        &provider_name,
    )?;

    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let registry = Registry::with_builtin();
        let rows = registry.asr().provider(provider_name.clone()).list();
        let descriptors: Vec<_> = rows.iter().map(|entry| &entry.descriptor).collect();
        let entry = select_asr_model(
            &descriptors,
            &provider_name,
            model.as_deref(),
            AsrMode::Streaming,
            None,
        )
        .map(|index| rows[index])
        .map_err(super::error::protocol_error)?;
        let model = model.unwrap_or_else(|| entry.descriptor.model.to_string());
        let handle = entry
            .instantiate(&ProviderConfig {
                provider: provider_name.clone(),
                model: model.clone(),
                api_key: Some(key),
                api_url,
                max_tokens: None,
                options: serde_json::Value::Null,
            })
            .map_err(super::error::protocol_error)?;
        let client_finalize = handle.descriptor().client_finalize();
        let realtime = handle
            .start_stream(StreamingTranscribeRequest {
                format,
                language: language.map(Language),
                options,
            })
            .await
            .map_err(super::error::protocol_error)?;
        let input = Arc::new(Mutex::new(Some(realtime.input)));
        let completion = Arc::new(Completion {
            result: Mutex::new(None),
            notify: Notify::new(),
        });
        let input_task = Arc::clone(&input);
        let completion_task = Arc::clone(&completion);
        tokio::spawn(async move {
            let mut events = realtime.events;
            let mut result = Ok(());
            while let Some(event) = events.next().await {
                let fatal_error = match &event {
                    StreamEvent::Error {
                        error, fatal: true, ..
                    } => Some(super::error::ProviderErrorData::from(error.clone())),
                    _ => None,
                };
                if let Some(event) = sdk_asr_event(&event) {
                    let event_json = match serde_json::to_string(&event) {
                        Ok(value) => value,
                        Err(error) => {
                            result = Err(StreamFailure::Runtime(error.to_string()));
                            break;
                        }
                    };
                    let callback_result = Python::attach(|py| -> PyResult<()> {
                        let value = py.import("json")?.call_method1("loads", (event_json,))?;
                        on_event.call1(py, (value,))?;
                        Ok(())
                    });
                    if let Err(error) = callback_result {
                        result = Err(StreamFailure::Runtime(error.to_string()));
                        break;
                    }
                }
                if let Some(error) = fatal_error {
                    result = Err(StreamFailure::Provider(error));
                    break;
                }
            }
            if result.is_ok() {
                match input_task.lock() {
                    Ok(input) if input.is_some() => {
                        result = Err(StreamFailure::Provider(
                            ProtocolError::new(
                                ErrorCode::ProviderStreamError,
                                "ASR event stream closed before finish",
                            )
                            .into(),
                        ));
                    }
                    Err(_) => {
                        result = Err(StreamFailure::Runtime(
                            "ASR session lock poisoned".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            if let Ok(mut slot) = completion_task.result.lock() {
                *slot = Some(result);
            }
            completion_task.notify.notify_waiters();
        });
        Ok(NativeAsrStream {
            input,
            completion,
            client_finalize,
            provider: provider_name,
            model,
        })
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
