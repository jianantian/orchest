use bytes::Bytes;
use orchest_protocol::{AudioFormat, ErrorCode, Language, ProtocolError, TranscribeRequest};
use orchest_provider::{ProviderConfig, Registry};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

const DEFAULT_PROVIDER: &str = "aliyun";
const DEFAULT_MODEL: &str = "qwen-audio-3.0-asr-flash";

#[pyfunction]
#[pyo3(signature = (audio, format, language=None, provider=None, model=None, api_key=None, api_key_env=None, api_url=None, options=None))]
#[allow(clippy::too_many_arguments)] // justified: pyo3 function maps Python kwargs 1:1
pub fn transcribe(
    py: Python<'_>,
    audio: Vec<u8>,
    format: &str,
    language: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    options: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    if audio.is_empty() {
        return Err(PyRuntimeError::new_err("audio cannot be empty"));
    }
    let (provider, model) = match (provider, model) {
        (None, None) => (
            DEFAULT_PROVIDER.to_string(),
            Some(DEFAULT_MODEL.to_string()),
        ),
        (Some(provider), model) => (validate_provider(provider)?, model),
        (None, Some(_)) => return Err(PyRuntimeError::new_err("model requires provider")),
    };
    let key = resolve_key(
        api_key,
        api_key_env.as_deref(),
        default_key_env(&provider),
        &provider,
    )?;
    let registry = Registry::with_builtin();
    let query = registry.asr().provider(provider.clone());
    let entry = match &model {
        Some(model) => query.id(&format!("{provider}/{model}")),
        None => query,
    }
    .select()
    .map_err(super::error::protocol_error)?;
    let model = model.unwrap_or_else(|| entry.descriptor.model.to_string());
    let handle = entry
        .instantiate(&ProviderConfig {
            provider,
            model,
            api_key: Some(key),
            api_url,
            max_tokens: None,
            options: Value::Null,
        })
        .map_err(super::error::protocol_error)?;
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
            .map_err(super::error::protocol_error)
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

/// `provider` is a bare provider name; `provider/model` is rejected so the two
/// stay separate parameters (ADR-0006, SDK 0.2).
pub(crate) fn validate_provider(provider: String) -> PyResult<String> {
    if provider.is_empty() || provider.contains('/') {
        return Err(PyRuntimeError::new_err(format!(
            "provider must be a provider name such as 'aliyun'; pass the model separately (got '{provider}')"
        )));
    }
    Ok(provider)
}

pub(crate) fn default_key_env(provider: &str) -> &'static str {
    match provider {
        "assemblyai" => "ASSEMBLYAI_API_KEY",
        "speechmatics" => "SPEECHMATICS_API_KEY",
        "deepgram" => "DEEPGRAM_API_KEY",
        "elevenlabs" => "ELEVENLABS_API_KEY",
        "soniox" => "SONIOX_API_KEY",
        _ => "DASHSCOPE_API_KEY",
    }
}

pub(crate) fn resolve_key(
    explicit: Option<String>,
    requested_env: Option<&str>,
    default_env: &str,
    provider_id: &str,
) -> PyResult<String> {
    if let Some(key) = explicit.filter(|key| !key.is_empty()) {
        return Ok(key);
    }
    let env = requested_env.unwrap_or(default_env);
    std::env::var(env).map_err(|_| {
        super::error::protocol_error(
            ProtocolError::new(
                ErrorCode::MissingApiKey,
                format!("API key environment variable {env} is not set"),
            )
            .with_provider(provider_id),
        )
    })
}
