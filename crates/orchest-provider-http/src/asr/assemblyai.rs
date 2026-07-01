//! AssemblyAI batch speech-to-text on the spine (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/assemblyai`, over the spine
//! [`ProtocolError`] / [`TranscribeResult`].
//!
//! AssemblyAI is a **batch REST** dialect (not WebSocket): upload the audio bytes
//! → submit a transcript job by `audio_url` → poll until `completed`. It lives in
//! the REST tier (`orchest-provider-http`) and implements the one-shot
//! [`Asr::transcribe`]; streaming is unsupported.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, Language, Modality, ProtocolError,
    RealtimeHandle, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};
use orchest_provider_core::registry::ProviderConfig;
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "https://api.assemblyai.com/v2";

/// AssemblyAI batch-ASR configuration.
#[derive(Debug, Clone)]
pub struct AssemblyAiConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: String,
    pub upload_url: String,
    pub poll_interval: Duration,
    pub max_polls: u32,
}

#[derive(Deserialize)]
struct UploadResponse {
    upload_url: String,
}

#[derive(Deserialize)]
struct SubmitResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptResponse {
    status: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("AssemblyAI HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("AssemblyAI HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_err(what: &str, e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse AssemblyAI {what} response: {e}"),
    )
}

/// Build the `/transcript` submit body. Spine `options` (a JSON object) supplies
/// `punctuate` / `format_text` / `speaker_labels` / `speech_model`; `language`
/// selects `language_code` (or `language_detection` when absent / `"auto"`).
pub fn build_submit_body(model: &str, request: &TranscribeRequest, audio_url: &str) -> Value {
    let opt = |key: &str| request.options.get(key);
    let mut body = json!({
        "audio_url": audio_url,
        "speech_model": model,
        "punctuate": opt("punctuate").and_then(Value::as_bool).unwrap_or(true),
        "format_text": opt("format_text").and_then(Value::as_bool).unwrap_or(true),
        "speaker_labels": opt("speaker_labels").and_then(Value::as_bool).unwrap_or(false),
    });
    match request.language.as_ref() {
        Some(lang) if lang.0 == "auto" => {
            body["language_detection"] = json!(true);
        }
        Some(lang) => {
            body["language_code"] = json!(lang.0);
        }
        None => {
            body["language_detection"] = json!(opt("language_detection")
                .and_then(Value::as_bool)
                .unwrap_or(true));
        }
    }
    if let Some(model_override) = opt("speech_model").and_then(Value::as_str) {
        body["speech_model"] = json!(model_override);
    }
    body
}

/// Project a completed transcript onto the spine result (text + detected language).
fn map_response(response: TranscriptResponse) -> TranscribeResult {
    let language = response.language_code.clone().map(Language);
    TranscribeResult {
        text: response.text.unwrap_or_default(),
        language,
        diagnostic_metadata: json!({ "provider": "assemblyai", "status": response.status }),
    }
}

/// The AssemblyAI batch ASR provider as the spine [`Asr`].
pub struct AssemblyAiAsr {
    config: AssemblyAiConfig,
}

impl AssemblyAiAsr {
    pub fn new(config: AssemblyAiConfig) -> Self {
        Self { config }
    }

    async fn upload_audio(&self, data: Bytes) -> Result<String, ProtocolError> {
        if data.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidAudio,
                "audio input cannot be empty",
            ));
        }
        let response = crate::http::shared_client()
            .post(&self.config.upload_url)
            .header("authorization", &self.config.api_key)
            .header("content-type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let body = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), body));
        }
        let parsed: UploadResponse =
            serde_json::from_str(&body).map_err(|e| parse_err("upload", e))?;
        Ok(parsed.upload_url)
    }

    async fn submit(
        &self,
        request: &TranscribeRequest,
        audio_url: &str,
    ) -> Result<String, ProtocolError> {
        let body = build_submit_body(&self.config.model, request, audio_url);
        let response = crate::http::shared_client()
            .post(format!("{}/transcript", self.config.api_url))
            .header("authorization", &self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        let parsed: SubmitResponse =
            serde_json::from_str(&text).map_err(|e| parse_err("submit", e))?;
        Ok(parsed.id)
    }

    async fn poll(&self, id: &str) -> Result<TranscriptResponse, ProtocolError> {
        for _ in 0..self.config.max_polls {
            let response = crate::http::shared_client()
                .get(format!("{}/transcript/{}", self.config.api_url, id))
                .header("authorization", &self.config.api_key)
                .send()
                .await
                .map_err(http_err)?;
            let status = response.status();
            let text = response.text().await.map_err(http_err)?;
            if !status.is_success() {
                return Err(status_err(status.as_u16(), text));
            }
            let parsed: TranscriptResponse =
                serde_json::from_str(&text).map_err(|e| parse_err("transcript", e))?;
            match parsed.status.as_str() {
                "completed" => return Ok(parsed),
                "error" => {
                    return Err(ProtocolError::new(
                        ErrorCode::ProviderTaskFailed,
                        parsed
                            .error
                            .unwrap_or_else(|| "AssemblyAI task failed".to_string()),
                    ))
                }
                _ => tokio::time::sleep(self.config.poll_interval).await,
            }
        }
        Err(ProtocolError::new(
            ErrorCode::Timeout,
            "AssemblyAI transcript did not complete before max poll count",
        ))
    }
}

/// The static descriptor the registry filters on for the assemblyai dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("assemblyai", "universal", Capability::Asr)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build an [`AssemblyAiAsr`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<AssemblyAiAsr, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "assemblyai requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "universal".to_string()
    } else {
        cfg.model.clone()
    };
    let api_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());
    let upload_url = format!("{api_url}/upload");
    Ok(AssemblyAiAsr::new(AssemblyAiConfig {
        model,
        api_key,
        api_url,
        upload_url,
        poll_interval: Duration::from_secs(2),
        max_polls: 60,
    }))
}

#[async_trait]
impl Asr for AssemblyAiAsr {
    fn provider_name(&self) -> &str {
        "assemblyai"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("assemblyai", self.config.model.clone(), Capability::Asr)
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
        let audio_url = self.upload_audio(request.audio.clone()).await?;
        let id = self.submit(&request, &audio_url).await?;
        let response = self.poll(&id).await?;
        Ok(map_response(response))
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "assemblyai ASR is batch-only; streaming is unsupported",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::AudioFormat;

    fn request(language: Option<&str>, options: Value) -> TranscribeRequest {
        TranscribeRequest {
            audio: Bytes::from_static(b"pcm"),
            format: AudioFormat::Wav,
            language: language.map(|l| Language(l.to_string())),
            options,
        }
    }

    #[test]
    fn submit_body_selects_language_code_or_detection() {
        let with_lang =
            build_submit_body("universal", &request(Some("en"), json!({})), "https://a/u");
        assert_eq!(with_lang["language_code"], "en");
        assert_eq!(with_lang["audio_url"], "https://a/u");
        assert_eq!(with_lang["speech_model"], "universal");

        let auto = build_submit_body("universal", &request(Some("auto"), json!({})), "u");
        assert_eq!(auto["language_detection"], true);
        assert!(auto.get("language_code").is_none());

        let none = build_submit_body("universal", &request(None, json!({})), "u");
        assert_eq!(none["language_detection"], true);
    }

    #[test]
    fn submit_body_honors_options() {
        let body = build_submit_body(
            "universal",
            &request(
                None,
                json!({"speaker_labels": true, "format_text": false, "speech_model": "best"}),
            ),
            "u",
        );
        assert_eq!(body["speaker_labels"], true);
        assert_eq!(body["format_text"], false);
        assert_eq!(body["speech_model"], "best");
    }

    #[test]
    fn maps_completed_response_to_text_and_language() {
        let response = TranscriptResponse {
            status: "completed".to_string(),
            text: Some("hello world".to_string()),
            language_code: Some("en".to_string()),
            error: None,
        };
        let result = map_response(response);
        assert_eq!(result.text, "hello world");
        assert_eq!(result.language, Some(Language("en".to_string())));
        assert_eq!(result.diagnostic_metadata["provider"], "assemblyai");
    }
}
