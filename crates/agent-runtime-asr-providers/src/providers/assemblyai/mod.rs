use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AsrError, AsrErrorCode};
use crate::observability::AsrTelemetryBuilder;
use crate::streaming::AsrStream;
use crate::traits::AsrProvider;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct AssemblyAiAsrConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: String,
    pub upload_url: String,
    pub poll_interval: Duration,
    pub max_polls: u32,
}

impl AssemblyAiAsrConfig {
    pub fn universal(api_key: impl Into<String>) -> Self {
        Self {
            model: "universal".into(),
            api_key: api_key.into(),
            api_url: "https://api.assemblyai.com/v2".into(),
            upload_url: "https://api.assemblyai.com/v2/upload".into(),
            poll_interval: Duration::from_secs(2),
            max_polls: 60,
        }
    }
}

pub struct AssemblyAiAsrAdapter {
    config: AssemblyAiAsrConfig,
}

impl AssemblyAiAsrAdapter {
    pub fn new(config: AssemblyAiAsrConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AsrProvider for AssemblyAiAsrAdapter {
    fn provider_name(&self) -> &str {
        "assemblyai"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        assemblyai_capabilities()
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        let started_at = Instant::now();
        let trace_id = request
            .options
            .trace_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let model = format!("assemblyai/{}", self.config.model);
        let audio_url = self.resolve_audio_url(&request).await?;
        let transcript_id = self.submit_transcript(&request, &audio_url).await?;
        let response = self.poll_transcript(&transcript_id).await?;
        Ok(map_response_to_result(
            response,
            &trace_id,
            &model,
            started_at.elapsed(),
            request.options.language.clone(),
        ))
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        Err(AsrError::unsupported_operation()
            .with_model(format!("assemblyai/{}", self.config.model)))
    }
}

fn assemblyai_capabilities() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![
            Language::new("auto"),
            Language::new("en"),
            Language::new("es"),
            Language::new("fr"),
            Language::new("de"),
            Language::new("it"),
            Language::new("pt"),
            Language::new("ja"),
            Language::new("ko"),
            Language::new("zh"),
        ],
        streaming: false,
        batch: true,
        streaming_inputs: vec![],
        batch_inputs: vec![
            batch_input(AudioFormat::Wav),
            batch_input(AudioFormat::Mp3),
            batch_input(AudioFormat::Flac),
            batch_input(AudioFormat::Ogg),
            batch_input(AudioFormat::Opus),
            batch_input(AudioFormat::Pcm),
        ],
        batch_format_inference: true,
        audio_timeline_modes: vec![],
        interim_results: false,
        endpointing_modes: vec![],
        segment_flush: false,
        multi_segment_streaming: false,
        connection_reuse: ConnectionReuse::NotReusable,
        word_timestamps: true,
        speaker_diarization: true,
        confidence: true,
        code_switching: false,
        hot_words: true,
        context_prompt: false,
        provider_option_keys: vec![
            "speaker_labels",
            "language_detection",
            "language_confidence_threshold",
            "speech_model",
            "format_text",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        max_duration_ms: None,
        default_flush_timeout_ms: None,
        source: CapabilitySource::Static,
        diagnostic_metadata: json!({
            "provider": "assemblyai",
            "transcribe": "batch_submit_poll",
            "streaming": "unsupported"
        }),
    }
}

fn batch_input(format: AudioFormat) -> AudioInputCapability {
    AudioInputCapability {
        format,
        sample_rates_hz: SampleRateSupport::Any,
        channels: ChannelSupport::Any,
        max_duration_ms: None,
        max_bytes: None,
    }
}

impl AssemblyAiAsrAdapter {
    async fn resolve_audio_url(&self, request: &TranscribeRequest) -> Result<String, AsrError> {
        match &request.audio {
            AudioInput::Url { url, .. } => Ok(url.clone()),
            AudioInput::Bytes { data, .. } => self.upload_audio(data.clone()).await,
            AudioInput::File { path, .. } => {
                let data = tokio::fs::read(path).await.map_err(|e| {
                    AsrError::new(
                        AsrErrorCode::InvalidAudio,
                        format!("failed to read audio file '{}': {e}", path.display()),
                    )
                })?;
                self.upload_audio(data).await
            }
        }
    }

    async fn upload_audio(&self, data: Vec<u8>) -> Result<String, AsrError> {
        if data.is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::InvalidAudio,
                "audio input cannot be empty",
            ));
        }
        let response = crate::http::shared_client()
            .post(&self.config.upload_url)
            .header("authorization", &self.config.api_key)
            .header("content-type", "application/octet-stream")
            .body(data)
            .send()
            .await
            .map_err(provider_http_error)?;
        let status = response.status();
        let body = response.text().await.map_err(provider_http_error)?;
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), body));
        }
        let parsed: AssemblyUploadResponse = serde_json::from_str(&body).map_err(|e| {
            AsrError::new(
                AsrErrorCode::ProviderHttpError,
                format!("failed to parse AssemblyAI upload response: {e}"),
            )
            .with_upstream(
                Some(status.as_u16()),
                None,
                None,
                Some(json!({ "body": body })),
            )
        })?;
        Ok(parsed.upload_url)
    }

    async fn submit_transcript(
        &self,
        request: &TranscribeRequest,
        audio_url: &str,
    ) -> Result<String, AsrError> {
        let body = build_submit_body(&self.config.model, request, audio_url);
        let response = crate::http::shared_client()
            .post(join_url(&self.config.api_url, "/transcript"))
            .header("authorization", &self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(provider_http_error)?;
        let status = response.status();
        let text = response.text().await.map_err(provider_http_error)?;
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), text));
        }
        let parsed: AssemblySubmitResponse = serde_json::from_str(&text).map_err(|e| {
            AsrError::new(
                AsrErrorCode::ProviderHttpError,
                format!("failed to parse AssemblyAI submit response: {e}"),
            )
        })?;
        Ok(parsed.id)
    }

    async fn poll_transcript(&self, id: &str) -> Result<AssemblyTranscriptResponse, AsrError> {
        for _ in 0..self.config.max_polls {
            let response = crate::http::shared_client()
                .get(join_url(&self.config.api_url, &format!("/transcript/{id}")))
                .header("authorization", &self.config.api_key)
                .send()
                .await
                .map_err(provider_http_error)?;
            let status = response.status();
            let text = response.text().await.map_err(provider_http_error)?;
            if !status.is_success() {
                return Err(http_status_error(status.as_u16(), text));
            }
            let parsed: AssemblyTranscriptResponse = serde_json::from_str(&text).map_err(|e| {
                AsrError::new(
                    AsrErrorCode::ProviderHttpError,
                    format!("failed to parse AssemblyAI transcript response: {e}"),
                )
            })?;
            match parsed.status.as_str() {
                "completed" => return Ok(parsed),
                "error" => {
                    return Err(AsrError::new(
                        AsrErrorCode::ProviderTaskFailed,
                        parsed
                            .error
                            .unwrap_or_else(|| "AssemblyAI task failed".into()),
                    ))
                }
                _ => tokio::time::sleep(self.config.poll_interval).await,
            }
        }
        Err(AsrError::new(
            AsrErrorCode::Timeout,
            "AssemblyAI transcript did not complete before max poll count",
        ))
    }
}

fn build_submit_body(model: &str, request: &TranscribeRequest, audio_url: &str) -> Value {
    let mut body = json!({
        "audio_url": audio_url,
        "speech_model": model,
        "punctuate": request.options.punctuate,
        "format_text": request.provider_options.get("format_text").and_then(Value::as_bool).unwrap_or(true),
        "speaker_labels": request.provider_options.get("speaker_labels").and_then(Value::as_bool).unwrap_or(request.options.speaker_diarization),
        "word_boost": request.options.hot_words,
    });

    if let Some(language) = &request.options.language {
        if language.as_str() == "auto" {
            body["language_detection"] = json!(true);
        } else {
            body["language_code"] = json!(language.as_str());
        }
    } else {
        body["language_detection"] = json!(request
            .provider_options
            .get("language_detection")
            .and_then(Value::as_bool)
            .unwrap_or(true));
    }
    if let Some(threshold) = request
        .provider_options
        .get("language_confidence_threshold")
        .and_then(Value::as_f64)
    {
        body["language_confidence_threshold"] = json!(threshold);
    }
    if let Some(model_override) = request
        .provider_options
        .get("speech_model")
        .and_then(Value::as_str)
    {
        body["speech_model"] = json!(model_override);
    }
    body
}

fn map_response_to_result(
    response: AssemblyTranscriptResponse,
    trace_id: &str,
    model: &str,
    latency: Duration,
    requested_language: Option<Language>,
) -> TranscribeResult {
    let words = response
        .words
        .into_iter()
        .map(|w| WordTimestamp {
            word: w.text,
            start_ms: w.start,
            end_ms: w.end,
            confidence: w.confidence,
        })
        .collect::<Vec<_>>();
    let speakers = response
        .utterances
        .unwrap_or_default()
        .into_iter()
        .filter_map(|u| {
            Some(SpeakerSegment {
                speaker_id: u.speaker?,
                start_ms: u.start,
                end_ms: u.end,
                text: u.text,
            })
        })
        .collect::<Vec<_>>();
    let language = response
        .language_code
        .map(Language::new)
        .or(requested_language);
    let duration_ms = response
        .audio_duration
        .map(|seconds| (seconds * 1000.0) as u64)
        .or_else(|| words.iter().map(|w| w.end_ms).max())
        .unwrap_or(0);
    let mut telemetry = AsrTelemetryBuilder::new(trace_id.into(), model.into());
    telemetry.set_audio_duration_ms(duration_ms);
    if let Some(language) = &language {
        telemetry.set_language(language.as_str().into());
    }
    if let Some(confidence) = response.confidence {
        telemetry.on_transcript_update(Some(confidence), false);
    }
    let text = response.text.unwrap_or_default();
    TranscribeResult {
        language,
        confidence: response.confidence,
        words,
        speakers,
        audio_duration_ms: duration_ms,
        processing_latency_ms: latency.as_millis() as u64,
        usage: AsrUsage {
            audio_duration_ms: duration_ms,
            billable_duration_ms: Some(duration_ms),
            input_bytes: None,
            transcript_chars: Some(text.chars().count() as u64),
            cost_estimate_micros: None,
        },
        option_adjustments: vec![],
        telemetry: telemetry.build(),
        text,
    }
}

fn provider_http_error(error: reqwest::Error) -> AsrError {
    AsrError::new(
        AsrErrorCode::ProviderHttpError,
        format!("AssemblyAI HTTP request failed: {error}"),
    )
}

fn http_status_error(status: u16, body: String) -> AsrError {
    let parsed = serde_json::from_str::<Value>(&body).ok();
    AsrError::new(
        AsrErrorCode::ProviderHttpError,
        format!("AssemblyAI HTTP request failed with status {status}"),
    )
    .with_upstream(Some(status), None, None, parsed)
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

#[derive(Debug, Deserialize)]
struct AssemblyUploadResponse {
    upload_url: String,
}

#[derive(Debug, Deserialize)]
struct AssemblySubmitResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AssemblyTranscriptResponse {
    status: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    audio_duration: Option<f64>,
    #[serde(default)]
    words: Vec<AssemblyWord>,
    #[serde(default)]
    utterances: Option<Vec<AssemblyUtterance>>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssemblyWord {
    text: String,
    start: u64,
    end: u64,
    #[serde(default)]
    confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AssemblyUtterance {
    text: String,
    start: u64,
    end: u64,
    #[serde(default)]
    speaker: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TranscribeRequest {
        TranscribeRequest {
            model: Some("assemblyai/universal".into()),
            audio: AudioInput::Url {
                url: "https://example.com/audio.wav".into(),
                format: Some(AudioFormat::Wav),
            },
            options: TranscribeOptions {
                language: Some(Language::new("en")),
                speaker_diarization: true,
                word_timestamps: true,
                hot_words: vec!["orchest".into()],
                ..Default::default()
            },
            timeout: None,
            compatibility: CompatibilityPolicy::Strict,
            provider_options: json!({"format_text": true}),
        }
    }

    #[test]
    fn capabilities_are_batch_only_with_metadata() {
        let caps = assemblyai_capabilities();
        assert!(caps.batch);
        assert!(!caps.streaming);
        assert!(caps.word_timestamps);
        assert!(caps.speaker_diarization);
        assert!(caps.confidence);
    }

    #[test]
    fn submit_body_maps_metadata_options() {
        let body = build_submit_body("universal", &request(), "https://example.com/audio.wav");
        assert_eq!(body["audio_url"], "https://example.com/audio.wav");
        assert_eq!(body["speech_model"], "universal");
        assert_eq!(body["language_code"], "en");
        assert_eq!(body["speaker_labels"], true);
        assert_eq!(body["word_boost"][0], "orchest");
    }

    #[test]
    fn response_maps_words_speakers_and_usage() {
        let parsed: AssemblyTranscriptResponse = serde_json::from_str(
            r#"{
                "status": "completed",
                "text": "hello world",
                "language_code": "en",
                "confidence": 0.93,
                "audio_duration": 1.5,
                "words": [
                    {"text":"hello","start":0,"end":500,"confidence":0.91},
                    {"text":"world","start":600,"end":1200,"confidence":0.95}
                ],
                "utterances": [
                    {"speaker":"A","start":0,"end":1200,"text":"hello world"}
                ]
            }"#,
        )
        .unwrap();
        let result = map_response_to_result(
            parsed,
            "trace-1",
            "assemblyai/universal",
            Duration::from_millis(25),
            None,
        );
        assert_eq!(result.text, "hello world");
        assert_eq!(result.language, Some(Language::new("en")));
        assert_eq!(result.confidence, Some(0.93));
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.speakers[0].speaker_id, "A");
        assert_eq!(result.audio_duration_ms, 1500);
        assert_eq!(result.usage.transcript_chars, Some(11));
    }

    #[tokio::test]
    async fn start_stream_returns_unsupported_operation() {
        let adapter = AssemblyAiAsrAdapter::new(AssemblyAiAsrConfig::universal("key"));
        let err = match adapter
            .start_stream(StreamingTranscribeRequest {
                model: Some("assemblyai/universal".into()),
                format: StreamingAudioFormat::Pcm16 {
                    sample_rate_hz: 16000,
                    channels: 1,
                },
                timeline: AudioTimelineMode::ContinuousRealtime,
                options: TranscribeOptions::default(),
                compatibility: CompatibilityPolicy::Coerce,
                provider_options: Value::Null,
            })
            .await
        {
            Err(err) => err,
            Ok(_) => panic!("start_stream should be unsupported"),
        };
        assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
    }
}
