use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AsrError, AsrErrorCode};
use crate::observability::AsrTelemetryBuilder;
use crate::streaming::AsrStream;
use crate::traits::AsrProvider;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct SpeechmaticsAsrConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: String,
    pub poll_interval: Duration,
    pub max_polls: u32,
}

impl SpeechmaticsAsrConfig {
    pub fn enhanced(api_key: impl Into<String>) -> Self {
        Self {
            model: "enhanced".into(),
            api_key: api_key.into(),
            api_url: "https://asr.api.speechmatics.com/v2".into(),
            poll_interval: Duration::from_secs(2),
            max_polls: 60,
        }
    }
}

pub struct SpeechmaticsAsrAdapter {
    config: SpeechmaticsAsrConfig,
}

impl SpeechmaticsAsrAdapter {
    pub fn new(config: SpeechmaticsAsrConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AsrProvider for SpeechmaticsAsrAdapter {
    fn provider_name(&self) -> &str {
        "speechmatics"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        speechmatics_capabilities(&self.config.model)
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
        let model = format!("speechmatics/{}", self.config.model);
        let form = self.build_job_form(&request).await?;
        let job_id = self.submit_job(form).await?;
        let duration_ms = self.poll_job(&job_id).await?;
        let transcript = self.fetch_transcript(&job_id).await?;
        Ok(map_transcript_to_result(
            transcript,
            SpeechmaticsResultContext {
                trace_id: &trace_id,
                model: &model,
                latency: started_at.elapsed(),
                duration_ms_hint: duration_ms,
                requested_language: request.options.language.clone(),
            },
        ))
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        Err(AsrError::unsupported_operation()
            .with_model(format!("speechmatics/{}", self.config.model)))
    }
}

fn speechmatics_capabilities(model: &str) -> AsrModelCapabilities {
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
            "operating_point",
            "diarization",
            "additional_vocab",
            "enable_entities",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        max_duration_ms: None,
        default_flush_timeout_ms: None,
        source: CapabilitySource::Static,
        diagnostic_metadata: json!({
            "provider": "speechmatics",
            "model": model,
            "transcribe": "batch_submit_poll_transcript",
            "streaming": "unsupported",
            "url_input": "unsupported_in_this_adapter"
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

impl SpeechmaticsAsrAdapter {
    async fn build_job_form(&self, request: &TranscribeRequest) -> Result<Form, AsrError> {
        let config = build_job_config(&self.config.model, request);
        let config_part = Part::text(config.to_string())
            .mime_str("application/json")
            .map_err(|e| {
                AsrError::new(
                    AsrErrorCode::InvalidRequest,
                    format!("failed to build Speechmatics config part: {e}"),
                )
            })?;
        let audio_part = match &request.audio {
            AudioInput::Bytes { data, .. } => {
                if data.is_empty() {
                    return Err(AsrError::new(
                        AsrErrorCode::InvalidAudio,
                        "audio input cannot be empty",
                    ));
                }
                Part::bytes(data.clone()).file_name("audio.bin")
            }
            AudioInput::File { path, .. } => {
                let data = tokio::fs::read(path).await.map_err(|e| {
                    AsrError::new(
                        AsrErrorCode::InvalidAudio,
                        format!("failed to read audio file '{}': {e}", path.display()),
                    )
                })?;
                if data.is_empty() {
                    return Err(AsrError::new(
                        AsrErrorCode::InvalidAudio,
                        "audio input cannot be empty",
                    ));
                }
                Part::bytes(data).file_name(
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("audio.bin")
                        .to_string(),
                )
            }
            AudioInput::Url { .. } => {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedOperation,
                    "Speechmatics adapter supports file and byte batch inputs; URL fetch is not implemented in this iteration",
                )
                .with_model(format!("speechmatics/{}", self.config.model)))
            }
        };
        Ok(Form::new()
            .part("config", config_part)
            .part("data_file", audio_part))
    }

    async fn submit_job(&self, form: Form) -> Result<String, AsrError> {
        let response = crate::http::shared_client()
            .post(join_url(&self.config.api_url, "/jobs"))
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(provider_http_error)?;
        let status = response.status();
        let text = response.text().await.map_err(provider_http_error)?;
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), text));
        }
        parse_job_id(&text).ok_or_else(|| {
            AsrError::new(
                AsrErrorCode::ProviderHttpError,
                "Speechmatics create job response did not include a job id",
            )
            .with_upstream(
                Some(status.as_u16()),
                None,
                None,
                serde_json::from_str(&text).ok(),
            )
        })
    }

    async fn poll_job(&self, id: &str) -> Result<u64, AsrError> {
        for _ in 0..self.config.max_polls {
            let response = crate::http::shared_client()
                .get(join_url(&self.config.api_url, &format!("/jobs/{id}")))
                .bearer_auth(&self.config.api_key)
                .send()
                .await
                .map_err(provider_http_error)?;
            let status = response.status();
            let text = response.text().await.map_err(provider_http_error)?;
            if !status.is_success() {
                return Err(http_status_error(status.as_u16(), text));
            }
            let job = parse_job_status(&text)?;
            match job.status.as_str() {
                "done" => return Ok(job.duration_ms.unwrap_or(0)),
                "rejected" | "failed" => {
                    return Err(AsrError::new(
                        AsrErrorCode::ProviderTaskFailed,
                        job.error
                            .unwrap_or_else(|| "Speechmatics job failed".into()),
                    ))
                }
                _ => tokio::time::sleep(self.config.poll_interval).await,
            }
        }
        Err(AsrError::new(
            AsrErrorCode::Timeout,
            "Speechmatics job did not complete before max poll count",
        ))
    }

    async fn fetch_transcript(&self, id: &str) -> Result<SpeechmaticsTranscript, AsrError> {
        let response = crate::http::shared_client()
            .get(join_url(
                &self.config.api_url,
                &format!("/jobs/{id}/transcript?format=json-v2"),
            ))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(provider_http_error)?;
        let status = response.status();
        let text = response.text().await.map_err(provider_http_error)?;
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), text));
        }
        serde_json::from_str(&text).map_err(|e| {
            AsrError::new(
                AsrErrorCode::ProviderHttpError,
                format!("failed to parse Speechmatics transcript response: {e}"),
            )
        })
    }
}

fn build_job_config(model: &str, request: &TranscribeRequest) -> Value {
    let language = request
        .options
        .language
        .as_ref()
        .map(|l| l.as_str())
        .unwrap_or("auto");
    let operating_point = request
        .provider_options
        .get("operating_point")
        .and_then(Value::as_str)
        .unwrap_or(model);
    let diarization = request
        .provider_options
        .get("diarization")
        .and_then(Value::as_str)
        .or_else(|| request.options.speaker_diarization.then_some("speaker"));
    let mut transcription_config = json!({
        "language": language,
        "operating_point": operating_point,
        "enable_partials": false,
    });
    if let Some(diarization) = diarization {
        transcription_config["diarization"] = json!(diarization);
    }
    let vocab = additional_vocab(request);
    if !vocab.is_empty() {
        transcription_config["additional_vocab"] = json!(vocab);
    }
    if let Some(enable_entities) = request
        .provider_options
        .get("enable_entities")
        .and_then(Value::as_bool)
    {
        transcription_config["enable_entities"] = json!(enable_entities);
    }
    json!({
        "type": "transcription",
        "transcription_config": transcription_config
    })
}

fn additional_vocab(request: &TranscribeRequest) -> Vec<Value> {
    let mut vocab = request
        .options
        .hot_words
        .iter()
        .map(|word| json!({"content": word}))
        .collect::<Vec<_>>();
    if let Some(extra) = request
        .provider_options
        .get("additional_vocab")
        .and_then(Value::as_array)
    {
        vocab.extend(extra.iter().cloned());
    }
    vocab
}

struct SpeechmaticsResultContext<'a> {
    trace_id: &'a str,
    model: &'a str,
    latency: Duration,
    duration_ms_hint: u64,
    requested_language: Option<Language>,
}

fn map_transcript_to_result(
    transcript: SpeechmaticsTranscript,
    context: SpeechmaticsResultContext<'_>,
) -> TranscribeResult {
    let mut text = String::new();
    let mut words = Vec::new();
    let mut speakers = Vec::new();
    let mut current_speaker: Option<SpeakerSegment> = None;
    let mut confidences = Vec::new();
    let mut detected_language = None;

    for item in transcript.results {
        let Some(alt) = item.alternatives.first() else {
            continue;
        };
        if item.kind.as_deref() == Some("punctuation") {
            text.push_str(&alt.content);
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&alt.content);
        if let Some(confidence) = alt.confidence {
            confidences.push(confidence);
        }
        if detected_language.is_none() {
            detected_language = alt.language.clone();
        }
        let start_ms = seconds_to_ms(item.start_time.unwrap_or(0.0));
        let end_ms = seconds_to_ms(item.end_time.unwrap_or(item.start_time.unwrap_or(0.0)));
        words.push(WordTimestamp {
            word: alt.content.clone(),
            start_ms,
            end_ms,
            confidence: alt.confidence,
        });
        if let Some(speaker) = &alt.speaker {
            match current_speaker.as_mut() {
                Some(segment) if segment.speaker_id == *speaker => {
                    segment.end_ms = end_ms;
                    if !segment.text.is_empty() {
                        segment.text.push(' ');
                    }
                    segment.text.push_str(&alt.content);
                }
                Some(_) => {
                    if let Some(segment) = current_speaker.replace(SpeakerSegment {
                        speaker_id: speaker.clone(),
                        start_ms,
                        end_ms,
                        text: alt.content.clone(),
                    }) {
                        speakers.push(segment);
                    }
                }
                None => {
                    current_speaker = Some(SpeakerSegment {
                        speaker_id: speaker.clone(),
                        start_ms,
                        end_ms,
                        text: alt.content.clone(),
                    });
                }
            }
        }
    }
    if let Some(segment) = current_speaker {
        speakers.push(segment);
    }

    let confidence = average(&confidences);
    let duration_ms = context
        .duration_ms_hint
        .max(words.iter().map(|w| w.end_ms).max().unwrap_or(0));
    let language = detected_language
        .map(Language::new)
        .or(context.requested_language);
    let mut telemetry = AsrTelemetryBuilder::new(context.trace_id.into(), context.model.into());
    telemetry.set_audio_duration_ms(duration_ms);
    if let Some(language) = &language {
        telemetry.set_language(language.as_str().into());
    }
    if let Some(confidence) = confidence {
        telemetry.on_transcript_update(Some(confidence), false);
    }
    TranscribeResult {
        text,
        language,
        confidence,
        words,
        speakers,
        audio_duration_ms: duration_ms,
        processing_latency_ms: context.latency.as_millis() as u64,
        usage: AsrUsage {
            audio_duration_ms: duration_ms,
            billable_duration_ms: Some(duration_ms),
            input_bytes: None,
            transcript_chars: None,
            cost_estimate_micros: None,
        },
        option_adjustments: vec![],
        telemetry: telemetry.build(),
    }
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn parse_job_id(text: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    value
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| value.get("job")?.get("id")?.as_str())
        .map(String::from)
}

fn parse_job_status(text: &str) -> Result<SpeechmaticsJobStatus, AsrError> {
    let value = serde_json::from_str::<Value>(text).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderHttpError,
            format!("failed to parse Speechmatics job response: {e}"),
        )
    })?;
    let job = value.get("job").unwrap_or(&value);
    let status = job
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let duration_ms = job
        .get("duration")
        .and_then(Value::as_f64)
        .map(seconds_to_ms);
    let error = job
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| job.get("message").and_then(Value::as_str))
        .map(String::from);
    Ok(SpeechmaticsJobStatus {
        status,
        duration_ms,
        error,
    })
}

fn seconds_to_ms(seconds: f64) -> u64 {
    (seconds * 1000.0).round() as u64
}

fn provider_http_error(error: reqwest::Error) -> AsrError {
    AsrError::new(
        AsrErrorCode::ProviderHttpError,
        format!("Speechmatics HTTP request failed: {error}"),
    )
}

fn http_status_error(status: u16, body: String) -> AsrError {
    AsrError::new(
        AsrErrorCode::ProviderHttpError,
        format!("Speechmatics HTTP request failed with status {status}"),
    )
    .with_upstream(Some(status), None, None, serde_json::from_str(&body).ok())
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

#[derive(Debug)]
struct SpeechmaticsJobStatus {
    status: String,
    duration_ms: Option<u64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpeechmaticsTranscript {
    #[serde(default)]
    results: Vec<SpeechmaticsResultItem>,
}

#[derive(Debug, Deserialize)]
struct SpeechmaticsResultItem {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    start_time: Option<f64>,
    #[serde(default)]
    end_time: Option<f64>,
    #[serde(default)]
    alternatives: Vec<SpeechmaticsAlternative>,
}

#[derive(Debug, Deserialize)]
struct SpeechmaticsAlternative {
    content: String,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TranscribeRequest {
        TranscribeRequest {
            model: Some("speechmatics/enhanced".into()),
            audio: AudioInput::Bytes {
                data: vec![0, 1, 2],
                format: AudioFormat::Wav,
                sample_rate_hz: None,
            },
            options: TranscribeOptions {
                language: Some(Language::new("en")),
                speaker_diarization: true,
                hot_words: vec!["orchest".into()],
                ..Default::default()
            },
            timeout: None,
            compatibility: CompatibilityPolicy::Strict,
            provider_options: json!({"enable_entities": true}),
        }
    }

    #[test]
    fn capabilities_are_batch_multilingual() {
        let caps = speechmatics_capabilities("enhanced");
        assert!(caps.batch);
        assert!(!caps.streaming);
        assert!(caps.speaker_diarization);
        assert!(caps.languages.contains(&Language::new("auto")));
    }

    #[test]
    fn job_config_maps_language_diarization_and_vocab() {
        let config = build_job_config("enhanced", &request());
        let tc = &config["transcription_config"];
        assert_eq!(tc["language"], "en");
        assert_eq!(tc["operating_point"], "enhanced");
        assert_eq!(tc["diarization"], "speaker");
        assert_eq!(tc["additional_vocab"][0]["content"], "orchest");
        assert_eq!(tc["enable_entities"], true);
    }

    #[test]
    fn transcript_maps_words_speakers_confidence_and_punctuation() {
        let transcript: SpeechmaticsTranscript = serde_json::from_str(
            r#"{
                "results": [
                    {"type":"word","start_time":0.0,"end_time":0.5,"alternatives":[{"content":"hello","confidence":0.8,"language":"en","speaker":"S1"}]},
                    {"type":"word","start_time":0.6,"end_time":1.0,"alternatives":[{"content":"world","confidence":0.9,"language":"en","speaker":"S1"}]},
                    {"type":"punctuation","alternatives":[{"content":"."}]}
                ]
            }"#,
        )
        .unwrap();
        let result = map_transcript_to_result(
            transcript,
            SpeechmaticsResultContext {
                trace_id: "trace-1",
                model: "speechmatics/enhanced",
                latency: Duration::from_millis(30),
                duration_ms_hint: 0,
                requested_language: None,
            },
        );
        assert_eq!(result.text, "hello world.");
        assert_eq!(result.language, Some(Language::new("en")));
        assert_eq!(result.confidence, Some(0.8500000000000001));
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.speakers.len(), 1);
        assert_eq!(result.speakers[0].speaker_id, "S1");
        assert_eq!(result.audio_duration_ms, 1000);
    }

    #[test]
    fn parse_job_status_accepts_nested_shape() {
        let status =
            parse_job_status(r#"{"job":{"id":"abc","status":"done","duration":2.25}}"#).unwrap();
        assert_eq!(status.status, "done");
        assert_eq!(status.duration_ms, Some(2250));
    }

    #[tokio::test]
    async fn url_input_is_explicitly_unsupported() {
        let adapter = SpeechmaticsAsrAdapter::new(SpeechmaticsAsrConfig::enhanced("key"));
        let mut request = request();
        request.audio = AudioInput::Url {
            url: "https://example.com/audio.wav".into(),
            format: Some(AudioFormat::Wav),
        };
        let err = adapter.transcribe(request).await.unwrap_err();
        assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
    }

    #[tokio::test]
    async fn start_stream_returns_unsupported_operation() {
        let adapter = SpeechmaticsAsrAdapter::new(SpeechmaticsAsrConfig::enhanced("key"));
        let err = match adapter
            .start_stream(StreamingTranscribeRequest {
                model: Some("speechmatics/enhanced".into()),
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
