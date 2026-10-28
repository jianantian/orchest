use bytes::Bytes;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use orchest::bindings::asr::{select_asr_model, AsrMode};
use orchest_protocol::{AudioFormat, ErrorCode, Language, ProtocolError, TranscribeRequest};
use orchest_provider::{ProviderConfig, Registry};

const DEFAULT_PROVIDER: &str = "aliyun";
const DEFAULT_MODEL: &str = "qwen-audio-3.0-asr-flash";

#[napi(object)]
pub struct TranscribeOptions {
    pub format: String,
    pub language: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub options: Option<serde_json::Value>,
}

#[napi(js_name = "_transcribe")]
pub async fn transcribe(audio: Buffer, input: TranscribeOptions) -> napi::Result<String> {
    if audio.is_empty() {
        return Err(napi::Error::from_reason("audio cannot be empty"));
    }
    let (provider, model) = match (input.provider, input.model) {
        (None, None) => (
            DEFAULT_PROVIDER.to_string(),
            Some(DEFAULT_MODEL.to_string()),
        ),
        (Some(provider), model) => (validate_provider(provider)?, model),
        (None, Some(_)) => return Err(napi::Error::from_reason("model requires provider")),
    };
    let key = resolve_key(
        input.api_key,
        input.api_key_env.as_deref(),
        default_key_env(&provider),
        &provider,
    )?;
    let registry = Registry::with_builtin();
    let rows = registry.asr().provider(provider.clone()).list();
    let descriptors: Vec<_> = rows.iter().map(|entry| &entry.descriptor).collect();
    let entry = select_asr_model(
        &descriptors,
        &provider,
        model.as_deref(),
        AsrMode::Batch,
        (provider == DEFAULT_PROVIDER).then_some(DEFAULT_MODEL),
    )
    .map(|index| rows[index])
    .map_err(super::error::protocol_error)?;
    let model = model.unwrap_or_else(|| entry.descriptor.model.to_string());
    let handle = entry
        .instantiate(&ProviderConfig {
            provider,
            model,
            api_key: Some(key),
            api_url: input.api_url,
            max_tokens: None,
            options: serde_json::Value::Null,
        })
        .map_err(super::error::protocol_error)?;
    handle
        .transcribe(TranscribeRequest {
            audio: Bytes::copy_from_slice(&audio),
            format: parse_audio_format(&input.format)?,
            language: input.language.map(Language),
            options: input.options.unwrap_or(serde_json::Value::Null),
        })
        .await
        .map(|result| result.text)
        .map_err(super::error::protocol_error)
}

pub(crate) fn parse_audio_format(value: &str) -> napi::Result<AudioFormat> {
    match value {
        "m4a" => Ok(AudioFormat::M4a),
        "aac" => Ok(AudioFormat::Aac),
        "wav" => Ok(AudioFormat::Wav),
        "mp3" => Ok(AudioFormat::Mp3),
        "pcm" => Ok(AudioFormat::Pcm),
        _ => Err(napi::Error::from_reason(format!(
            "unknown audio format: {value}"
        ))),
    }
}
/// `provider` is a bare provider name; `provider/model` is rejected so the two
/// stay separate options (ADR-0006, SDK 0.2).
pub(crate) fn validate_provider(provider: String) -> napi::Result<String> {
    if provider.is_empty() || provider.contains('/') {
        return Err(napi::Error::from_reason(format!(
            "provider must be a provider name such as 'aliyun'; pass the model separately (got '{provider}')"
        )));
    }
    Ok(provider)
}

pub(crate) fn default_key_env(provider: &str) -> &'static str {
    orchest::bindings::asr::default_asr_key_env(provider)
}

pub(crate) fn resolve_key(
    explicit: Option<String>,
    requested_env: Option<&str>,
    default_env: &str,
    provider_id: &str,
) -> napi::Result<String> {
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
