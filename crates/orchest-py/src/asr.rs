use bytes::Bytes;
use orchest_protocol::{AudioFormat, Language, TranscribeRequest};
use orchest_provider::{ProviderConfig, Registry};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

const DEFAULT_ASR: &str = "aliyun/qwen-audio-3.0-asr-flash";

#[pyfunction]
#[pyo3(signature = (audio, format, language=None, provider=None, api_key=None, api_key_env=None, api_url=None, options=None))]
#[allow(clippy::too_many_arguments)] // justified: pyo3 function maps Python kwargs 1:1
pub fn transcribe(
    py: Python<'_>,
    audio: Vec<u8>,
    format: &str,
    language: Option<String>,
    provider: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    options: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    if audio.is_empty() {
        return Err(PyRuntimeError::new_err("audio cannot be empty"));
    }
    let id = provider.unwrap_or_else(|| DEFAULT_ASR.to_string());
    let key = resolve_key(api_key, api_key_env.as_deref(), default_key_env(&id))?;
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
            options: Value::Null,
        })
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    let options = py_dict_value(options)?;
    let request = TranscribeRequest {
        audio: Bytes::from(audio),
        format: parse_audio_format(format)?,
        language: language.map(Language),
        options,
    };
    py.detach(|| {
        tokio::runtime::Runtime::new()
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?
            .block_on(handle.transcribe(request))
            .map(|result| result.text)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    })
}

pub(crate) fn parse_audio_format(value: &str) -> PyResult<AudioFormat> {
    match value {
        "m4a" => Ok(AudioFormat::M4a),
        "aac" => Ok(AudioFormat::Aac),
        "wav" => Ok(AudioFormat::Wav),
        "mp3" => Ok(AudioFormat::Mp3),
        "pcm" => Ok(AudioFormat::Pcm),
        _ => Err(PyRuntimeError::new_err(format!(
            "unknown audio format: {value}"
        ))),
    }
}

pub(crate) fn py_dict_value(value: Option<Bound<'_, PyDict>>) -> PyResult<Value> {
    let Some(value) = value else {
        return Ok(Value::Null);
    };
    let json: String = value
        .py()
        .import("json")?
        .call_method1("dumps", (&value,))?
        .extract()?;
    serde_json::from_str(&json).map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

pub(crate) fn default_key_env(id: &str) -> &'static str {
    if id.starts_with("assemblyai/") {
        "ASSEMBLYAI_API_KEY"
    } else if id.starts_with("speechmatics/") {
        "SPEECHMATICS_API_KEY"
    } else {
        "DASHSCOPE_API_KEY"
    }
}

pub(crate) fn resolve_key(
    explicit: Option<String>,
    requested_env: Option<&str>,
    default_env: &str,
) -> PyResult<String> {
    if let Some(key) = explicit.filter(|key| !key.is_empty()) {
        return Ok(key);
    }
    let env = requested_env.unwrap_or(default_env);
    std::env::var(env).map_err(|_| {
        PyRuntimeError::new_err(format!("API key environment variable {env} is not set"))
    })
}
