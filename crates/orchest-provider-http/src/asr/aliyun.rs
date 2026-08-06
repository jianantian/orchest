//! Aliyun DashScope synchronous ASR over multimodal-generation HTTP.

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use orchest_protocol::{
    Asr, AudioFormat, Capability, CapabilityDescriptor, ErrorCode, Language, Modality,
    ProtocolError, RealtimeHandle, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};
use orchest_provider_core::aliyun_asr::{parse_context, ContextRole};
use orchest_provider_core::registry::ProviderConfig;
use serde_json::{json, Map, Value};

const DEFAULT_API_URL: &str =
    "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation";
const DEFAULT_MODEL: &str = "qwen-audio-3.0-asr-flash";

pub struct AliyunAsrConfig {
    pub model: String,
    pub api_url: String,
    pub api_key: String,
}

pub struct AliyunAsr {
    config: AliyunAsrConfig,
}

impl AliyunAsr {
    pub fn new(config: AliyunAsrConfig) -> Self {
        Self { config }
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries provider diagnostics by workspace convention
pub fn build_request_body(
    model: &str,
    request: &TranscribeRequest,
) -> Result<Value, ProtocolError> {
    if request.audio.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidAudio,
            "Aliyun ASR audio cannot be empty",
        ));
    }
    let (format, mime) = format_and_mime(request.format)?;
    let context = parse_context(&request.options)?;
    let mut parameters = match &request.options {
        Value::Null => Map::new(),
        Value::Object(object) => object.clone(),
        _ => {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "Aliyun ASR options must be an object",
            ))
        }
    };
    parameters.remove("context");
    parameters.insert("format".into(), json!(format));
    if let Some(sample_rate) = parameters.get_mut("sample_rate") {
        let normalized = match sample_rate {
            Value::Number(number) => number.to_string(),
            Value::String(value) => value.clone(),
            _ => {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    "Aliyun ASR sample_rate must be an integer or decimal string",
                ))
            }
        };
        *sample_rate = Value::String(normalized);
    }
    if let Some(language) = &request.language {
        parameters.insert("language_hints".into(), json!([language.0]));
    }

    let mut messages = context
        .into_iter()
        .map(|message| match message.role {
            ContextRole::User => json!({
                "role": "user",
                "content": [{"type": "input_text", "text": message.text}]
            }),
            ContextRole::Assistant => json!({
                "role": "assistant",
                "content": [{"type": "text", "text": message.text}]
            }),
        })
        .collect::<Vec<_>>();
    let data = format!("data:{mime};base64,{}", STANDARD.encode(&request.audio));
    messages.push(json!({
        "role": "user",
        "content": [{"type": "input_audio", "input_audio": {"data": data}}]
    }));

    Ok(json!({
        "model": model,
        "input": {"messages": messages},
        "parameters": Value::Object(parameters),
    }))
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries provider diagnostics by workspace convention
pub fn parse_response(value: Value) -> Result<TranscribeResult, ProtocolError> {
    let text = value
        .pointer("/output/text")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::ProviderHttpError,
                "Aliyun ASR response is missing output.text",
            )
        })?
        .to_string();
    Ok(TranscribeResult {
        text,
        language: None,
        diagnostic_metadata: json!({
            "request_id": value.get("request_id").cloned().unwrap_or(Value::Null),
            "usage": value.get("usage").cloned().unwrap_or(Value::Null),
            "sentence": value.pointer("/output/sentence").cloned().unwrap_or(Value::Null),
        }),
    })
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries provider diagnostics by workspace convention
fn format_and_mime(format: AudioFormat) -> Result<(&'static str, &'static str), ProtocolError> {
    match format {
        AudioFormat::M4a => Ok(("m4a", "audio/mp4")),
        AudioFormat::Aac => Ok(("aac", "audio/aac")),
        AudioFormat::Wav | AudioFormat::WavPcm16Le => Ok(("wav", "audio/wav")),
        AudioFormat::Mp3 => Ok(("mp3", "audio/mpeg")),
        AudioFormat::Pcm | AudioFormat::Pcm16Le => Ok(("pcm", "audio/pcm")),
        _ => Err(ProtocolError::new(
            ErrorCode::UnsupportedAudioFormat,
            "audio format is not supported by Aliyun synchronous ASR",
        )),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries provider diagnostics by workspace convention
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<AliyunAsr, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .ok_or_else(|| ProtocolError::new(ErrorCode::MissingApiKey, "aliyun requires api_key"))?;
    Ok(AliyunAsr::new(AliyunAsrConfig {
        model: if cfg.model.is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            cfg.model.clone()
        },
        api_url: cfg
            .api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.to_string()),
        api_key,
    }))
}

#[async_trait]
impl Asr for AliyunAsr {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("aliyun", self.config.model.clone(), Capability::Asr)
            .with_input_modalities([Modality::Audio])
            .with_output_modalities([Modality::Text])
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResult, ProtocolError> {
        let response = crate::http::shared_client()
            .post(&self.config.api_url)
            .bearer_auth(&self.config.api_key)
            .header("X-DashScope-SSE", "disable")
            .json(&build_request_body(&self.config.model, &request)?)
            .send()
            .await
            .map_err(|error| {
                ProtocolError::new(
                    ErrorCode::ProviderHttpError,
                    format!("Aliyun ASR request failed: {error}"),
                )
                .with_provider("aliyun")
                .with_model(self.config.model.clone())
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProtocolError::new(
                ErrorCode::ProviderHttpError,
                format!("Aliyun ASR request returned HTTP {status}"),
            )
            .with_provider("aliyun")
            .with_model(self.config.model.clone())
            .with_status(status.as_u16()));
        }
        let value = response.json::<Value>().await.map_err(|error| {
            ProtocolError::new(
                ErrorCode::ProviderHttpError,
                format!("failed to parse Aliyun ASR response: {error}"),
            )
            .with_provider("aliyun")
            .with_model(self.config.model.clone())
        })?;
        parse_response(value)
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "Aliyun HTTP ASR does not support streaming",
        ))
    }
}
