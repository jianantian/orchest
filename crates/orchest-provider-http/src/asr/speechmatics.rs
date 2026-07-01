//! Speechmatics batch speech-to-text on the spine (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/speechmatics`, over the spine
//! [`ProtocolError`] / [`TranscribeResult`].
//!
//! Speechmatics is a **batch REST** dialect: submit a job (multipart: a JSON
//! `config` part + the audio `data_file`) → poll `/jobs/{id}` until `done` →
//! fetch the `json-v2` transcript and assemble its word/punctuation results into
//! text. REST tier; one-shot [`Asr::transcribe`] only.

use std::time::Duration;

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, Language, Modality, ProtocolError,
    RealtimeHandle, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};
use orchest_provider_core::registry::ProviderConfig;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "https://asr.api.speechmatics.com/v2";

/// Speechmatics batch-ASR configuration.
#[derive(Debug, Clone)]
pub struct SpeechmaticsConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: String,
    pub poll_interval: Duration,
    pub max_polls: u32,
}

#[derive(Debug, Deserialize)]
struct Transcript {
    #[serde(default)]
    results: Vec<ResultItem>,
}

#[derive(Debug, Deserialize)]
struct ResultItem {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    content: String,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug)]
struct JobStatus {
    status: String,
    error: Option<String>,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Speechmatics HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Speechmatics HTTP {code}: {body}"),
    )
    .with_status(code)
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

/// Build the job `config` object. `language` selects the transcription language
/// (`"auto"` when absent); spine `options` supply `operating_point` / `diarization`.
pub fn build_job_config(model: &str, request: &TranscribeRequest) -> Value {
    let language = request
        .language
        .as_ref()
        .map(|l| l.0.as_str())
        .unwrap_or("auto");
    let operating_point = request
        .options
        .get("operating_point")
        .and_then(Value::as_str)
        .unwrap_or(model);
    let mut transcription_config = json!({
        "language": language,
        "operating_point": operating_point,
        "enable_partials": false,
    });
    if let Some(diarization) = request.options.get("diarization").and_then(Value::as_str) {
        transcription_config["diarization"] = json!(diarization);
    }
    json!({ "type": "transcription", "transcription_config": transcription_config })
}

/// Assemble the `json-v2` results into plain text + the first detected language:
/// words are space-joined, punctuation is appended without a leading space.
fn assemble_transcript(results: Vec<ResultItem>) -> (String, Option<String>) {
    let mut text = String::new();
    let mut language = None;
    for item in results {
        let Some(alt) = item.alternatives.into_iter().next() else {
            continue;
        };
        if language.is_none() {
            language = alt.language.clone();
        }
        if item.kind.as_deref() == Some("punctuation") {
            text.push_str(&alt.content);
        } else {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&alt.content);
        }
    }
    (text, language)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_job_id(text: &str) -> Result<String, ProtocolError> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(ErrorCode::ProviderHttpError, format!("parse job id: {e}"))
    })?;
    value
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| value.get("job")?.get("id")?.as_str())
        .map(String::from)
        .ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::ProviderHttpError,
                "Speechmatics create-job response did not include a job id",
            )
        })
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_job_status(text: &str) -> Result<JobStatus, ProtocolError> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderHttpError,
            format!("parse job status: {e}"),
        )
    })?;
    let job = value.get("job").unwrap_or(&value);
    Ok(JobStatus {
        status: job
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        error: job
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| job.get("message").and_then(Value::as_str))
            .map(String::from),
    })
}

/// The Speechmatics batch ASR provider as the spine [`Asr`].
pub struct SpeechmaticsAsr {
    config: SpeechmaticsConfig,
}

impl SpeechmaticsAsr {
    pub fn new(config: SpeechmaticsConfig) -> Self {
        Self { config }
    }

    async fn submit_job(&self, config: Value, audio: Vec<u8>) -> Result<String, ProtocolError> {
        if audio.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidAudio,
                "audio input cannot be empty",
            ));
        }
        let form = Form::new()
            .part("config", Part::text(config.to_string()))
            .part("data_file", Part::bytes(audio).file_name("audio.bin"));
        let response = crate::http::shared_client()
            .post(join_url(&self.config.api_url, "/jobs"))
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        parse_job_id(&text)
    }

    async fn poll_job(&self, id: &str) -> Result<(), ProtocolError> {
        for _ in 0..self.config.max_polls {
            let response = crate::http::shared_client()
                .get(join_url(&self.config.api_url, &format!("/jobs/{id}")))
                .bearer_auth(&self.config.api_key)
                .send()
                .await
                .map_err(http_err)?;
            let status = response.status();
            let text = response.text().await.map_err(http_err)?;
            if !status.is_success() {
                return Err(status_err(status.as_u16(), text));
            }
            let job = parse_job_status(&text)?;
            match job.status.as_str() {
                "done" => return Ok(()),
                "rejected" | "failed" => {
                    return Err(ProtocolError::new(
                        ErrorCode::ProviderTaskFailed,
                        job.error
                            .unwrap_or_else(|| "Speechmatics job failed".to_string()),
                    ))
                }
                _ => tokio::time::sleep(self.config.poll_interval).await,
            }
        }
        Err(ProtocolError::new(
            ErrorCode::Timeout,
            "Speechmatics job did not complete before max poll count",
        ))
    }

    async fn fetch_transcript(&self, id: &str) -> Result<Transcript, ProtocolError> {
        let response = crate::http::shared_client()
            .get(join_url(
                &self.config.api_url,
                &format!("/jobs/{id}/transcript?format=json-v2"),
            ))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        serde_json::from_str(&text).map_err(|e| {
            ProtocolError::new(
                ErrorCode::ProviderHttpError,
                format!("failed to parse Speechmatics transcript: {e}"),
            )
        })
    }
}

/// The static descriptor the registry filters on for the speechmatics dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("speechmatics", "enhanced", Capability::Asr)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build a [`SpeechmaticsAsr`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<SpeechmaticsAsr, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "speechmatics requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "enhanced".to_string()
    } else {
        cfg.model.clone()
    };
    let api_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());
    Ok(SpeechmaticsAsr::new(SpeechmaticsConfig {
        model,
        api_key,
        api_url,
        poll_interval: Duration::from_secs(2),
        max_polls: 90,
    }))
}

#[async_trait]
impl Asr for SpeechmaticsAsr {
    fn provider_name(&self) -> &str {
        "speechmatics"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("speechmatics", self.config.model.clone(), Capability::Asr)
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
        let config = build_job_config(&self.config.model, &request);
        let id = self.submit_job(config, request.audio.to_vec()).await?;
        self.poll_job(&id).await?;
        let transcript = self.fetch_transcript(&id).await?;
        let (text, language) = assemble_transcript(transcript.results);
        Ok(TranscribeResult {
            text,
            language: language.map(Language),
            diagnostic_metadata: json!({ "provider": "speechmatics" }),
        })
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "speechmatics ASR is batch-only; streaming is unsupported",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
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
    fn job_config_selects_language_and_operating_point() {
        let cfg = build_job_config("enhanced", &request(Some("en"), json!({})));
        assert_eq!(cfg["type"], "transcription");
        assert_eq!(cfg["transcription_config"]["language"], "en");
        assert_eq!(cfg["transcription_config"]["operating_point"], "enhanced");

        let auto = build_job_config(
            "enhanced",
            &request(
                None,
                json!({"operating_point": "standard", "diarization": "speaker"}),
            ),
        );
        assert_eq!(auto["transcription_config"]["language"], "auto");
        assert_eq!(auto["transcription_config"]["operating_point"], "standard");
        assert_eq!(auto["transcription_config"]["diarization"], "speaker");
    }

    #[test]
    fn assembles_words_and_punctuation() {
        let transcript: Transcript = serde_json::from_value(json!({
            "results": [
                {"type": "word", "alternatives": [{"content": "hello", "language": "en"}]},
                {"type": "word", "alternatives": [{"content": "world"}]},
                {"type": "punctuation", "alternatives": [{"content": "."}]}
            ]
        }))
        .unwrap();
        let (text, language) = assemble_transcript(transcript.results);
        assert_eq!(text, "hello world.");
        assert_eq!(language.as_deref(), Some("en"));
    }

    #[test]
    fn parses_job_id_and_status_shapes() {
        assert_eq!(
            parse_job_id(&json!({"id": "abc"}).to_string()).unwrap(),
            "abc"
        );
        assert_eq!(
            parse_job_id(&json!({"job": {"id": "xyz"}}).to_string()).unwrap(),
            "xyz"
        );
        let status = parse_job_status(&json!({"job": {"status": "running"}}).to_string()).unwrap();
        assert_eq!(status.status, "running");
    }
}
