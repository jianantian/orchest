use bytes::Bytes;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use orchest_protocol::{AudioFormat, Language, TranscribeRequest};
use orchest_provider::{ProviderConfig, Registry};

const DEFAULT_ASR: &str = "aliyun/qwen-audio-3.0-asr-flash";

#[napi(object)]
pub struct TranscribeOptions {
    pub format: String,
    pub language: Option<String>,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub options: Option<serde_json::Value>,
}

#[napi]
pub async fn transcribe(audio: Buffer, input: TranscribeOptions) -> napi::Result<String> {
    if audio.is_empty() {
        return Err(napi::Error::from_reason("audio cannot be empty"));
    }
    let id = input.provider.unwrap_or_else(|| DEFAULT_ASR.to_string());
    let key = resolve_key(
        input.api_key,
        input.api_key_env.as_deref(),
        default_key_env(&id),
    )?;
    let registry = Registry::with_builtin();
    let entry = registry
        .asr()
        .id(&id)
        .select()
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let (provider, model) = id
        .split_once('/')
        .ok_or_else(|| napi::Error::from_reason("provider must be provider/model"))?;
    let handle = entry
        .instantiate(&ProviderConfig {
            provider: provider.into(),
            model: model.into(),
            api_key: Some(key),
            api_url: input.api_url,
            max_tokens: None,
            options: serde_json::Value::Null,
        })
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    handle
        .transcribe(TranscribeRequest {
            audio: Bytes::copy_from_slice(&audio),
            format: parse_audio_format(&input.format)?,
            language: input.language.map(Language),
            options: input.options.unwrap_or(serde_json::Value::Null),
        })
        .await
        .map(|result| result.text)
        .map_err(|error| napi::Error::from_reason(error.to_string()))
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
) -> napi::Result<String> {
    if let Some(key) = explicit.filter(|key| !key.is_empty()) {
        return Ok(key);
    }
    let env = requested_env.unwrap_or(default_env);
    std::env::var(env).map_err(|_| {
        napi::Error::from_reason(format!("API key environment variable {env} is not set"))
    })
}
