use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tracing::Instrument;

use crate::error::{AsrError, AsrErrorCode};
use crate::observability::{self, AsrTelemetryBuilder};
use crate::streaming::{AsrAudioSink, AsrEventStream, AsrStream};
use crate::traits::AsrProvider;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct DeepgramAsrConfig {
    /// Deepgram model identifier, for example "nova-3".
    pub model: String,
    /// WebSocket endpoint URL without query parameters.
    pub ws_url: String,
    pub api_key: String,
}

impl DeepgramAsrConfig {
    pub fn nova_3(api_key: impl Into<String>) -> Self {
        Self {
            model: "nova-3".into(),
            ws_url: "wss://api.deepgram.com/v1/listen".into(),
            api_key: api_key.into(),
        }
    }
}

pub struct DeepgramAsrAdapter {
    config: DeepgramAsrConfig,
}

impl DeepgramAsrAdapter {
    pub fn new(config: DeepgramAsrConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AsrProvider for DeepgramAsrAdapter {
    fn provider_name(&self) -> &str {
        "deepgram"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        deepgram_capabilities()
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, _request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        Err(AsrError::unsupported_operation().with_model(format!("deepgram/{}", self.config.model)))
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }

        let trace_id = request
            .options
            .trace_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let model = format!("deepgram/{}", self.config.model);
        let ws_url = build_ws_url(&self.config.ws_url, &self.config.model, &request)?;

        let ws_request = tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Token {}", self.config.api_key))
            .header("Host", extract_host(&self.config.ws_url))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| {
                AsrError::new(
                    AsrErrorCode::InvalidRequest,
                    format!("failed to build WebSocket request: {e}"),
                )
            })?;

        let span = observability::provider_stream_span(&trace_id, &model);
        let (ws_stream, _response) = tokio_tungstenite::connect_async(ws_request)
            .instrument(span)
            .await
            .map_err(|e| {
                AsrError::new(
                    AsrErrorCode::ProviderStreamError,
                    format!("WebSocket connection failed: {e}"),
                )
            })?;

        let (audio_tx, audio_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(64);
        let flush_timeout = request
            .options
            .flush_timeout
            .unwrap_or(Duration::from_secs(3));
        let final_result_scope = request.options.final_result_scope.clone();
        let language = request.options.language.clone();

        event_tx
            .send(AsrStreamEvent::RouteSelected {
                trace_id: trace_id.clone(),
                model: model.clone(),
            })
            .await
            .ok();

        tokio::spawn(async move {
            adapter_task(
                ws_stream,
                audio_rx,
                event_tx,
                trace_id,
                model,
                flush_timeout,
                final_result_scope,
                language,
            )
            .await;
        });

        Ok(AsrStream::new(
            AsrAudioSink::new(audio_tx),
            AsrEventStream::new(event_rx),
        ))
    }
}

fn deepgram_capabilities() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![
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
        streaming: true,
        batch: false,
        streaming_inputs: vec![AudioInputCapability {
            format: AudioFormat::Pcm,
            sample_rates_hz: SampleRateSupport::Range {
                min: 8000,
                max: 48000,
            },
            channels: ChannelSupport::Any,
            max_duration_ms: None,
            max_bytes: None,
        }],
        batch_inputs: vec![],
        batch_format_inference: false,
        audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
        interim_results: true,
        endpointing_modes: vec![
            EndpointingMode::ProviderDefault,
            EndpointingMode::AcousticSilence,
            EndpointingMode::ProviderDisabled,
        ],
        segment_flush: true,
        multi_segment_streaming: true,
        connection_reuse: ConnectionReuse::NotReusable,
        word_timestamps: true,
        speaker_diarization: false,
        confidence: true,
        code_switching: false,
        hot_words: true,
        context_prompt: false,
        provider_option_keys: vec![
            "smart_format",
            "numerals",
            "filler_words",
            "profanity_filter",
            "redact",
            "search",
            "replace",
            "keywords",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        max_duration_ms: None,
        default_flush_timeout_ms: Some(3000),
        source: CapabilitySource::Static,
        diagnostic_metadata: serde_json::json!({
            "provider": "deepgram",
            "streaming_endpoint": "wss://api.deepgram.com/v1/listen",
            "transcribe": "unsupported_realtime_only"
        }),
    }
}

fn build_ws_url(
    base_url: &str,
    model: &str,
    request: &StreamingTranscribeRequest,
) -> Result<String, AsrError> {
    let mut params = vec![
        ("model".to_string(), model.to_string()),
        (
            "interim_results".to_string(),
            request.options.interim_results.to_string(),
        ),
        (
            "punctuate".to_string(),
            request.options.punctuate.to_string(),
        ),
    ];

    match &request.format {
        StreamingAudioFormat::Pcm16 {
            sample_rate_hz,
            channels,
        } => {
            params.push(("encoding".into(), "linear16".into()));
            params.push(("sample_rate".into(), sample_rate_hz.to_string()));
            params.push(("channels".into(), channels.to_string()));
        }
        StreamingAudioFormat::Encoded { format } => {
            return Err(AsrError::new(
                AsrErrorCode::UnsupportedAudioFormat,
                format!(
                    "Deepgram adapter currently supports streaming PCM16 only; got {:?}",
                    format
                ),
            ));
        }
    }

    if let Some(language) = &request.options.language {
        params.push(("language".into(), language.as_str().to_string()));
    }

    if request.options.word_timestamps {
        params.push(("words".into(), "true".into()));
    }

    if !request.options.hot_words.is_empty() {
        for hot_word in &request.options.hot_words {
            params.push(("keywords".into(), hot_word.clone()));
        }
    }

    if let Some(endpointing) = &request.options.endpointing {
        match endpointing.mode {
            EndpointingMode::ProviderDefault => {}
            EndpointingMode::AcousticSilence => {
                let value = endpointing
                    .silence_timeout
                    .map(|d| d.as_millis().to_string())
                    .unwrap_or_else(|| "true".into());
                params.push(("endpointing".into(), value));
            }
            EndpointingMode::ProviderDisabled => {
                params.push(("endpointing".into(), "false".into()));
            }
            EndpointingMode::Semantic | EndpointingMode::NaturalSegmenting => {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedOption,
                    format!(
                        "Deepgram adapter does not support {:?} endpointing",
                        endpointing.mode
                    ),
                ));
            }
        }
    }

    if let Some(map) = request.provider_options.as_object() {
        for (key, value) in map {
            if let Some(s) = value.as_str() {
                params.push((key.clone(), s.to_string()));
            } else if value.is_boolean() || value.is_number() {
                params.push((key.clone(), value.to_string()));
            }
        }
    }

    let separator = if base_url.contains('?') { '&' } else { '?' };
    let query = params
        .into_iter()
        .map(|(key, value)| format!("{}={}", query_encode(&key), query_encode(&value)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!("{base_url}{separator}{query}"))
}

fn query_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn extract_host(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split('/').next())
        .unwrap_or("api.deepgram.com")
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // justified: single-function duplex WebSocket adapter; splitting would scatter the stream state machine across helpers
async fn adapter_task(
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut audio_rx: mpsc::Receiver<AudioChunk>,
    event_tx: mpsc::Sender<AsrStreamEvent>,
    trace_id: String,
    model: String,
    flush_timeout: Duration,
    final_result_scope: FinalResultScope,
    language: Option<Language>,
) {
    let (mut ws_write, mut ws_read) = ws_stream.split();

    let _ = event_tx
        .send(AsrStreamEvent::Started {
            trace_id: trace_id.clone(),
            model: model.clone(),
        })
        .await;

    let mut telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
    telemetry.on_started();
    if let Some(language) = &language {
        telemetry.set_language(language.as_str().to_string());
    }

    let mut flush_pending = false;
    let mut flush_started: Option<std::time::Instant> = None;
    let mut end_requested = false;
    let mut accumulated_text = String::new();
    let mut segment_idx = 0u32;
    let mut last_segment_text = String::new();
    let mut audio_duration_ms = 0u64;
    let mut segment_words = Vec::new();

    loop {
        let flush_timeout_fut = if flush_pending {
            let elapsed = flush_started.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);
            if elapsed >= flush_timeout {
                tokio::time::sleep(Duration::ZERO)
            } else {
                tokio::time::sleep(flush_timeout - elapsed)
            }
        } else {
            tokio::time::sleep(Duration::from_secs(3600))
        };

        tokio::select! {
            chunk = audio_rx.recv() => {
                match chunk {
                    Some(chunk) => {
                        if !chunk.data.is_empty()
                            && ws_write.send(tungstenite::Message::Binary(chunk.data.to_vec())).await.is_err()
                        {
                            let _ = event_tx.send(AsrStreamEvent::Error {
                                trace_id: trace_id.clone(),
                                error: AsrError::new(AsrErrorCode::ProviderStreamError, "WebSocket send failed"),
                                fatal: true,
                            }).await;
                            return;
                        }

                        match chunk.boundary {
                            AudioChunkBoundary::None => {}
                            AudioChunkBoundary::Flush | AudioChunkBoundary::End => {
                                flush_pending = true;
                                flush_started = Some(std::time::Instant::now());
                                if matches!(chunk.boundary, AudioChunkBoundary::End) {
                                    end_requested = true;
                                }
                                if ws_write.send(tungstenite::Message::Text(r#"{"type":"Finalize"}"#.into())).await.is_err() {
                                    let _ = event_tx.send(AsrStreamEvent::Error {
                                        trace_id: trace_id.clone(),
                                        error: AsrError::new(AsrErrorCode::ProviderStreamError, "failed to send Deepgram Finalize"),
                                        fatal: true,
                                    }).await;
                                    return;
                                }
                            }
                        }
                    }
                    None => {
                        let _ = event_tx.send(AsrStreamEvent::Error {
                            trace_id: trace_id.clone(),
                            error: AsrError::new(AsrErrorCode::Cancelled, "audio sink dropped"),
                            fatal: true,
                        }).await;
                        return;
                    }
                }
            }
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        match parse_deepgram_message(&text) {
                            Ok(DeepgramParsedMessage::Result(result)) => {
                                if result.transcript.is_empty() {
                                    continue;
                                }
                                audio_duration_ms = result.audio_duration_ms.unwrap_or(audio_duration_ms);
                                if result.is_final {
                                    last_segment_text = result.transcript.clone();
                                    segment_words = result.words;
                                    let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                        trace_id: trace_id.clone(),
                                        segment_id: Some(format!("seg-{segment_idx}")),
                                        text: result.transcript.clone(),
                                        stability: TranscriptStability::Committed,
                                        update_kind: TranscriptUpdateKind::Append,
                                    }).await;
                                    telemetry.on_transcript_update(result.confidence, false);

                                    let should_finalize = flush_pending || result.speech_final;
                                    if should_finalize {
                                        if result.speech_final {
                                            let _ = event_tx.send(AsrStreamEvent::EndOfSpeech {
                                                trace_id: trace_id.clone(),
                                                segment_id: Some(format!("seg-{segment_idx}")),
                                            }).await;
                                        }

                                        let reason = if flush_pending {
                                            if end_requested {
                                                AsrFinalReason::CallerEnd
                                            } else {
                                                AsrFinalReason::CallerFlush
                                            }
                                        } else {
                                            AsrFinalReason::ProviderEndpoint
                                        };
                                        emit_final(
                                            &event_tx,
                                            &trace_id,
                                            segment_idx,
                                            &last_segment_text,
                                            &mut accumulated_text,
                                            &final_result_scope,
                                            reason,
                                            audio_duration_ms,
                                            std::mem::take(&mut segment_words),
                                            telemetry,
                                        ).await;

                                        if end_requested {
                                            let _ = ws_write.send(tungstenite::Message::Text(r#"{"type":"CloseStream"}"#.into())).await;
                                            let _ = ws_write.close().await;
                                            return;
                                        }

                                        flush_pending = false;
                                        segment_idx += 1;
                                        last_segment_text.clear();
                                        telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                                        telemetry.on_started();
                                        if let Some(language) = &language {
                                            telemetry.set_language(language.as_str().to_string());
                                        }
                                    }
                                } else {
                                    let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                        trace_id: trace_id.clone(),
                                        segment_id: Some(format!("seg-{segment_idx}")),
                                        text: result.transcript,
                                        stability: TranscriptStability::Provisional,
                                        update_kind: TranscriptUpdateKind::Snapshot,
                                    }).await;
                                    telemetry.on_transcript_update(result.confidence, false);
                                }
                            }
                            Ok(DeepgramParsedMessage::Ignored) => {}
                            Ok(DeepgramParsedMessage::Error(message)) => {
                                let _ = event_tx.send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error: AsrError::new(AsrErrorCode::ProviderStreamError, message),
                                    fatal: true,
                                }).await;
                                return;
                            }
                            Err(error) => {
                                let _ = event_tx.send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error,
                                    fatal: true,
                                }).await;
                                return;
                            }
                        }
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => return,
                    Some(Err(e)) => {
                        let _ = event_tx.send(AsrStreamEvent::Error {
                            trace_id: trace_id.clone(),
                            error: AsrError::new(AsrErrorCode::ProviderStreamError, format!("WebSocket error: {e}")),
                            fatal: true,
                        }).await;
                        return;
                    }
                    _ => {}
                }
            }
            _ = flush_timeout_fut => {
                if flush_pending {
                    emit_final(
                        &event_tx,
                        &trace_id,
                        segment_idx,
                        &last_segment_text,
                        &mut accumulated_text,
                        &final_result_scope,
                        AsrFinalReason::Timeout,
                        audio_duration_ms,
                        std::mem::take(&mut segment_words),
                        telemetry,
                    ).await;

                    if end_requested {
                        let _ = ws_write.send(tungstenite::Message::Text(r#"{"type":"CloseStream"}"#.into())).await;
                        let _ = ws_write.close().await;
                        return;
                    }

                    flush_pending = false;
                    segment_idx += 1;
                    last_segment_text.clear();
                    telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                    telemetry.on_started();
                    if let Some(language) = &language {
                        telemetry.set_language(language.as_str().to_string());
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // final assembly spans request state plus provider telemetry; keeping it local avoids shared mutable structs in the duplex loop
async fn emit_final(
    event_tx: &mpsc::Sender<AsrStreamEvent>,
    trace_id: &str,
    segment_idx: u32,
    last_segment_text: &str,
    accumulated_text: &mut String,
    final_result_scope: &FinalResultScope,
    reason: AsrFinalReason,
    audio_duration_ms: u64,
    words: Vec<WordTimestamp>,
    mut telemetry: AsrTelemetryBuilder,
) {
    let text = match final_result_scope {
        FinalResultScope::Stream => {
            if !accumulated_text.is_empty() && !last_segment_text.is_empty() {
                accumulated_text.push(' ');
            }
            accumulated_text.push_str(last_segment_text);
            accumulated_text.clone()
        }
        FinalResultScope::Segment => last_segment_text.to_string(),
    };

    telemetry.set_audio_duration_ms(audio_duration_ms);
    let telem = telemetry.build();
    observability::record_final_latency(&telem.model, telem.latency_final_ms);
    observability::record_audio_duration(&telem.model, audio_duration_ms);

    let _ = event_tx
        .send(AsrStreamEvent::AsrFinal {
            final_output: Box::new(AsrFinalOutput {
                trace_id: trace_id.to_string(),
                segment_id: Some(format!("seg-{segment_idx}")),
                reason,
                result: TranscribeResult {
                    text,
                    language: telem.language.as_ref().map(|l| Language::new(l.clone())),
                    confidence: telem.confidence_avg,
                    words,
                    speakers: vec![],
                    audio_duration_ms,
                    processing_latency_ms: telem.latency_final_ms,
                    usage: AsrUsage {
                        audio_duration_ms,
                        ..Default::default()
                    },
                    option_adjustments: vec![],
                    telemetry: telem,
                },
            }),
        })
        .await;
}

#[derive(Debug)]
enum DeepgramParsedMessage {
    Result(ParsedDeepgramResult),
    Error(String),
    Ignored,
}

#[derive(Debug)]
struct ParsedDeepgramResult {
    transcript: String,
    is_final: bool,
    speech_final: bool,
    confidence: Option<f64>,
    words: Vec<WordTimestamp>,
    audio_duration_ms: Option<u64>,
}

fn parse_deepgram_message(text: &str) -> Result<DeepgramParsedMessage, AsrError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("failed to parse Deepgram message: {e}"),
        )
    })?;
    let msg_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match msg_type {
        "Results" => parse_results(value).map(DeepgramParsedMessage::Result),
        "Error" => Ok(DeepgramParsedMessage::Error(
            value
                .get("description")
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Deepgram stream error")
                .to_string(),
        )),
        "Metadata" | "UtteranceEnd" | "SpeechStarted" | "KeepAlive" | "CloseStream" => {
            Ok(DeepgramParsedMessage::Ignored)
        }
        _ => Ok(DeepgramParsedMessage::Ignored),
    }
}

fn parse_results(value: serde_json::Value) -> Result<ParsedDeepgramResult, AsrError> {
    let message: DeepgramResultsMessage = serde_json::from_value(value).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("invalid Deepgram Results message: {e}"),
        )
    })?;
    let Some(alternative) = message.channel.alternatives.into_iter().next() else {
        return Ok(ParsedDeepgramResult {
            transcript: String::new(),
            is_final: message.is_final,
            speech_final: message.speech_final,
            confidence: None,
            words: vec![],
            audio_duration_ms: None,
        });
    };
    let words = alternative
        .words
        .into_iter()
        .map(|word| WordTimestamp {
            word: word.word,
            start_ms: seconds_to_ms(word.start),
            end_ms: seconds_to_ms(word.end),
            confidence: word.confidence,
        })
        .collect();
    Ok(ParsedDeepgramResult {
        transcript: alternative.transcript,
        is_final: message.is_final,
        speech_final: message.speech_final,
        confidence: alternative.confidence,
        words,
        audio_duration_ms: message
            .duration
            .or(message
                .start
                .map(|start| start + message.duration.unwrap_or(0.0)))
            .map(seconds_to_ms),
    })
}

fn seconds_to_ms(seconds: f64) -> u64 {
    if seconds <= 0.0 {
        0
    } else {
        (seconds * 1000.0).round() as u64
    }
}

#[derive(Debug, Deserialize)]
struct DeepgramResultsMessage {
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    #[serde(default)]
    start: Option<f64>,
    #[serde(default)]
    duration: Option<f64>,
    channel: DeepgramChannel,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    #[serde(default)]
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    words: Vec<DeepgramWord>,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    word: String,
    start: f64,
    end: f64,
    #[serde(default)]
    confidence: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StreamingTranscribeRequest {
        StreamingTranscribeRequest {
            model: Some("deepgram/nova-3".into()),
            format: StreamingAudioFormat::Pcm16 {
                sample_rate_hz: 16000,
                channels: 1,
            },
            timeline: AudioTimelineMode::ContinuousRealtime,
            options: TranscribeOptions {
                language: Some(Language::new("en")),
                word_timestamps: true,
                hot_words: vec!["orchest".into()],
                endpointing: Some(EndpointingOptions {
                    mode: EndpointingMode::AcousticSilence,
                    silence_timeout: Some(Duration::from_millis(750)),
                }),
                ..Default::default()
            },
            compatibility: CompatibilityPolicy::Strict,
            provider_options: serde_json::json!({"smart_format": true}),
        }
    }

    #[test]
    fn capabilities_are_realtime_only_and_include_endpointing() {
        let caps = deepgram_capabilities();
        assert!(caps.streaming);
        assert!(!caps.batch);
        assert!(!caps.batch_format_inference);
        assert!(caps.word_timestamps);
        assert!(caps.confidence);
        assert!(caps
            .endpointing_modes
            .contains(&EndpointingMode::AcousticSilence));
        assert!(caps
            .endpointing_modes
            .contains(&EndpointingMode::ProviderDisabled));
        assert!(caps
            .provider_option_keys
            .contains(&"smart_format".to_string()));
    }

    #[test]
    fn build_url_maps_pcm_and_options() {
        let url = build_ws_url("wss://api.deepgram.com/v1/listen", "nova-3", &request()).unwrap();
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("sample_rate=16000"));
        assert!(url.contains("channels=1"));
        assert!(url.contains("language=en"));
        assert!(url.contains("words=true"));
        assert!(url.contains("keywords=orchest"));
        assert!(url.contains("endpointing=750"));
        assert!(url.contains("smart_format=true"));
    }

    #[test]
    fn build_url_rejects_encoded_streams() {
        let mut request = request();
        request.format = StreamingAudioFormat::Encoded {
            format: AudioFormat::Mp3,
        };
        let err = build_ws_url("wss://api.deepgram.com/v1/listen", "nova-3", &request).unwrap_err();
        assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
    }

    #[test]
    fn parse_partial_result_to_provisional_update_data() {
        let parsed = parse_deepgram_message(
            r#"{
                "type": "Results",
                "is_final": false,
                "speech_final": false,
                "duration": 1.23,
                "channel": {
                    "alternatives": [{
                        "transcript": "hello wor",
                        "confidence": 0.8,
                        "words": []
                    }]
                }
            }"#,
        )
        .unwrap();
        match parsed {
            DeepgramParsedMessage::Result(result) => {
                assert_eq!(result.transcript, "hello wor");
                assert!(!result.is_final);
                assert_eq!(result.confidence, Some(0.8));
                assert_eq!(result.audio_duration_ms, Some(1230));
            }
            _ => panic!("expected result"),
        }
    }

    #[test]
    fn parse_final_result_with_words() {
        let parsed = parse_deepgram_message(
            r#"{
                "type": "Results",
                "is_final": true,
                "speech_final": true,
                "duration": 2.0,
                "channel": {
                    "alternatives": [{
                        "transcript": "hello world",
                        "confidence": 0.92,
                        "words": [
                            {"word": "hello", "start": 0.0, "end": 0.5, "confidence": 0.91},
                            {"word": "world", "start": 0.5, "end": 1.0, "confidence": 0.93}
                        ]
                    }]
                }
            }"#,
        )
        .unwrap();
        match parsed {
            DeepgramParsedMessage::Result(result) => {
                assert!(result.is_final);
                assert!(result.speech_final);
                assert_eq!(result.words.len(), 2);
                assert_eq!(result.words[1].start_ms, 500);
                assert_eq!(result.words[1].confidence, Some(0.93));
            }
            _ => panic!("expected result"),
        }
    }

    #[tokio::test]
    async fn transcribe_returns_unsupported_operation() {
        let adapter = DeepgramAsrAdapter::new(DeepgramAsrConfig::nova_3("key"));
        let err = adapter
            .transcribe(TranscribeRequest {
                model: Some("deepgram/nova-3".into()),
                audio: AudioInput::Bytes {
                    data: vec![0, 1],
                    format: AudioFormat::Pcm,
                    sample_rate_hz: Some(16000),
                },
                options: TranscribeOptions::default(),
                timeout: None,
                compatibility: CompatibilityPolicy::Coerce,
                provider_options: serde_json::Value::Null,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
        assert_eq!(err.model.as_deref(), Some("deepgram/nova-3"));
    }

    #[tokio::test]
    async fn deepgram_rejects_insecure_ws_url() {
        let adapter = DeepgramAsrAdapter::new(DeepgramAsrConfig {
            model: "nova-3".into(),
            ws_url: "ws://api.deepgram.com/v1/listen".into(),
            api_key: "key".into(),
        });
        let err = match adapter.start_stream(request()).await {
            Err(err) => err,
            Ok(_) => panic!("insecure WebSocket URL should be rejected before network connect"),
        };
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("wss://"));
    }
}
