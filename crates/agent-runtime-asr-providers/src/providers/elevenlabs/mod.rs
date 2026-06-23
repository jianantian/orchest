use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
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
pub struct ElevenLabsAsrConfig {
    /// ElevenLabs model identifier, for example "scribe_v2_realtime".
    pub model: String,
    /// WebSocket endpoint URL without query parameters.
    pub ws_url: String,
    pub api_key: String,
}

impl ElevenLabsAsrConfig {
    pub fn scribe_v2_realtime(api_key: impl Into<String>) -> Self {
        Self {
            model: "scribe_v2_realtime".into(),
            ws_url: "wss://api.elevenlabs.io/v1/speech-to-text/stream".into(),
            api_key: api_key.into(),
        }
    }
}

pub struct ElevenLabsAsrAdapter {
    config: ElevenLabsAsrConfig,
}

impl ElevenLabsAsrAdapter {
    pub fn new(config: ElevenLabsAsrConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AsrProvider for ElevenLabsAsrAdapter {
    fn provider_name(&self) -> &str {
        "elevenlabs"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        elevenlabs_capabilities()
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, _request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        Err(AsrError::unsupported_operation()
            .with_model(format!("elevenlabs/{}", self.config.model)))
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        if request.options.speaker_diarization {
            return Err(AsrError::new(
                AsrErrorCode::UnsupportedOption,
                "speaker_diarization is not supported by ElevenLabs Scribe realtime",
            ));
        }
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
        let model = format!("elevenlabs/{}", self.config.model);
        let session = ElevenLabsSessionConfig::from_request(&request)?;
        let ws_url = build_ws_url(&self.config.ws_url, &self.config.model, &request, &session)?;

        let ws_request = tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("xi-api-key", &self.config.api_key)
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
        let expect_timestamps = request.options.word_timestamps;

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
                session,
                expect_timestamps,
            )
            .await;
        });

        Ok(AsrStream::new(
            AsrAudioSink::new(audio_tx),
            AsrEventStream::new(event_rx),
        ))
    }
}

fn elevenlabs_capabilities() -> AsrModelCapabilities {
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
            Language::new("auto"),
        ],
        streaming: true,
        batch: false,
        streaming_inputs: vec![AudioInputCapability {
            format: AudioFormat::Pcm,
            sample_rates_hz: SampleRateSupport::Exact(vec![
                8000, 16000, 22050, 24000, 44100, 48000,
            ]),
            channels: ChannelSupport::Exact(vec![1]),
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
        confidence: false,
        code_switching: false,
        hot_words: true,
        context_prompt: false,
        provider_option_keys: vec![
            "keyterms",
            "include_language_detection",
            "commit_strategy",
            "vad_silence_threshold_secs",
            "enable_logging",
            "tag_audio_events",
            "language_code",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        max_duration_ms: None,
        default_flush_timeout_ms: Some(3000),
        source: CapabilitySource::Static,
        diagnostic_metadata: serde_json::json!({
            "provider": "elevenlabs",
            "streaming_endpoint": "wss://api.elevenlabs.io/v1/speech-to-text/stream",
            "batch_model_reference": "elevenlabs/scribe_v2",
            "transcribe": "unsupported_realtime_only"
        }),
    }
}

#[derive(Debug, Clone)]
struct ElevenLabsSessionConfig {
    audio_format: String,
    sample_rate_hz: u32,
    include_language_detection: bool,
    commit_strategy: String,
    vad_silence_threshold_secs: Option<f64>,
    keyterms: Vec<String>,
}

impl ElevenLabsSessionConfig {
    fn from_request(request: &StreamingTranscribeRequest) -> Result<Self, AsrError> {
        let (audio_format, sample_rate_hz) = match &request.format {
            StreamingAudioFormat::Pcm16 {
                sample_rate_hz,
                channels,
            } => {
                if *channels != 1 {
                    return Err(AsrError::new(
                        AsrErrorCode::UnsupportedAudioFormat,
                        "ElevenLabs Scribe realtime currently supports mono PCM16 only",
                    ));
                }
                (pcm_audio_format(*sample_rate_hz)?, *sample_rate_hz)
            }
            StreamingAudioFormat::Encoded { format } => {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedAudioFormat,
                    format!(
                        "ElevenLabs adapter currently supports streaming PCM16 only; got {:?}",
                        format
                    ),
                ))
            }
        };

        let mut keyterms = request.options.hot_words.clone();
        if let Some(extra) = request.provider_options.get("keyterms") {
            if let Some(s) = extra.as_str() {
                keyterms.push(s.to_string());
            } else if let Some(values) = extra.as_array() {
                keyterms.extend(values.iter().filter_map(|v| v.as_str().map(String::from)));
            }
        }

        let include_language_detection = request
            .provider_options
            .get("include_language_detection")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| request.options.language.is_none());

        let mut commit_strategy = request
            .provider_options
            .get("commit_strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string();
        let mut vad_silence_threshold_secs = request
            .provider_options
            .get("vad_silence_threshold_secs")
            .and_then(|v| v.as_f64());

        if let Some(endpointing) = &request.options.endpointing {
            match endpointing.mode {
                EndpointingMode::ProviderDefault => {}
                EndpointingMode::ProviderDisabled => commit_strategy = "manual".into(),
                EndpointingMode::AcousticSilence => {
                    commit_strategy = "vad".into();
                    if let Some(timeout) = endpointing.silence_timeout {
                        vad_silence_threshold_secs = Some(timeout.as_secs_f64());
                    }
                }
                EndpointingMode::Semantic | EndpointingMode::NaturalSegmenting => {
                    return Err(AsrError::new(
                        AsrErrorCode::UnsupportedOption,
                        format!(
                            "ElevenLabs adapter does not support {:?} endpointing",
                            endpointing.mode
                        ),
                    ))
                }
            }
        }

        Ok(Self {
            audio_format,
            sample_rate_hz,
            include_language_detection,
            commit_strategy,
            vad_silence_threshold_secs,
            keyterms,
        })
    }
}

fn pcm_audio_format(sample_rate_hz: u32) -> Result<String, AsrError> {
    match sample_rate_hz {
        8000 | 16000 | 22050 | 24000 | 44100 | 48000 => Ok(format!("pcm_{sample_rate_hz}")),
        other => Err(AsrError::new(
            AsrErrorCode::UnsupportedAudioFormat,
            format!("unsupported ElevenLabs PCM sample rate: {other}Hz"),
        )),
    }
}

fn build_ws_url(
    base_url: &str,
    model: &str,
    request: &StreamingTranscribeRequest,
    session: &ElevenLabsSessionConfig,
) -> Result<String, AsrError> {
    let mut params = vec![
        ("model_id".to_string(), model.to_string()),
        ("audio_format".to_string(), session.audio_format.clone()),
        (
            "include_timestamps".to_string(),
            request.options.word_timestamps.to_string(),
        ),
        (
            "include_language_detection".to_string(),
            session.include_language_detection.to_string(),
        ),
        (
            "commit_strategy".to_string(),
            session.commit_strategy.clone(),
        ),
    ];

    if let Some(language) = request
        .provider_options
        .get("language_code")
        .and_then(|v| v.as_str())
        .or_else(|| request.options.language.as_ref().map(Language::as_str))
    {
        if language != "auto" {
            params.push(("language_code".into(), language.to_string()));
        }
    }

    if let Some(vad) = session.vad_silence_threshold_secs {
        params.push(("vad_silence_threshold_secs".into(), format_float(vad)));
    }

    for keyterm in &session.keyterms {
        params.push(("keyterms".into(), keyterm.clone()));
    }

    for key in ["enable_logging", "tag_audio_events"] {
        if let Some(value) = request.provider_options.get(key) {
            if value.is_boolean() || value.is_number() {
                params.push((key.to_string(), value.to_string()));
            } else if let Some(s) = value.as_str() {
                params.push((key.to_string(), s.to_string()));
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

fn format_float(value: f64) -> String {
    let mut formatted = format!("{value:.3}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
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
        .unwrap_or("api.elevenlabs.io")
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // justified: single-function duplex WebSocket adapter; splitting would scatter the session state machine across helpers
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
    session: ElevenLabsSessionConfig,
    expect_timestamps: bool,
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
    let mut segment_words = Vec::new();
    let mut audio_duration_ms = 0u64;

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
                        if !chunk.data.is_empty() {
                            let message = build_audio_message(&chunk.data, false, &session, None);
                            if ws_write.send(tungstenite::Message::Text(message)).await.is_err() {
                                let _ = event_tx.send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error: AsrError::new(AsrErrorCode::ProviderStreamError, "WebSocket send failed"),
                                    fatal: true,
                                }).await;
                                return;
                            }
                        }

                        match chunk.boundary {
                            AudioChunkBoundary::None => {}
                            AudioChunkBoundary::Flush | AudioChunkBoundary::End => {
                                flush_pending = true;
                                flush_started = Some(std::time::Instant::now());
                                if matches!(chunk.boundary, AudioChunkBoundary::End) {
                                    end_requested = true;
                                }
                                let message = build_audio_message(&[], true, &session, None);
                                if ws_write.send(tungstenite::Message::Text(message)).await.is_err() {
                                    let _ = event_tx.send(AsrStreamEvent::Error {
                                        trace_id: trace_id.clone(),
                                        error: AsrError::new(AsrErrorCode::ProviderStreamError, "failed to send ElevenLabs commit chunk"),
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
                        match parse_elevenlabs_message(&text) {
                            Ok(ElevenLabsParsedMessage::SessionStarted) | Ok(ElevenLabsParsedMessage::Ignored) => {}
                            Ok(ElevenLabsParsedMessage::Partial { transcript }) => {
                                if transcript.is_empty() {
                                    continue;
                                }
                                let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                    trace_id: trace_id.clone(),
                                    segment_id: Some(format!("seg-{segment_idx}")),
                                    text: transcript,
                                    stability: TranscriptStability::Provisional,
                                    update_kind: TranscriptUpdateKind::Snapshot,
                                }).await;
                                telemetry.on_transcript_update(None, false);
                            }
                            Ok(ElevenLabsParsedMessage::Committed { transcript }) => {
                                if transcript.is_empty() {
                                    continue;
                                }
                                last_segment_text = transcript.clone();
                                let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                    trace_id: trace_id.clone(),
                                    segment_id: Some(format!("seg-{segment_idx}")),
                                    text: transcript,
                                    stability: TranscriptStability::Committed,
                                    update_kind: TranscriptUpdateKind::Append,
                                }).await;
                                telemetry.on_transcript_update(None, false);

                                if !expect_timestamps {
                                    let reason = if end_requested { AsrFinalReason::CallerEnd } else { AsrFinalReason::CallerFlush };
                                    emit_final(
                                        &event_tx,
                                        &trace_id,
                                        segment_idx,
                                        &last_segment_text,
                                        &mut accumulated_text,
                                        &final_result_scope,
                                        reason,
                                        audio_duration_ms,
                                        Vec::new(),
                                        telemetry,
                                    ).await;
                                    if end_requested {
                                        let _ = ws_write.close().await;
                                        return;
                                    }
                                    flush_pending = false;
                                    segment_idx += 1;
                                    last_segment_text.clear();
                                    telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                                    telemetry.on_started();
                                }
                            }
                            Ok(ElevenLabsParsedMessage::CommittedWithTimestamps { transcript, words, language_code }) => {
                                if transcript.is_empty() {
                                    continue;
                                }
                                last_segment_text = transcript.clone();
                                segment_words = words;
                                audio_duration_ms = segment_words.iter().map(|w| w.end_ms).max().unwrap_or(audio_duration_ms);
                                if let Some(language_code) = language_code {
                                    telemetry.set_language(language_code);
                                }
                                let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                    trace_id: trace_id.clone(),
                                    segment_id: Some(format!("seg-{segment_idx}")),
                                    text: transcript,
                                    stability: TranscriptStability::Committed,
                                    update_kind: TranscriptUpdateKind::Append,
                                }).await;
                                telemetry.on_transcript_update(None, false);

                                let reason = if flush_pending {
                                    if end_requested { AsrFinalReason::CallerEnd } else { AsrFinalReason::CallerFlush }
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
                                    let _ = ws_write.close().await;
                                    return;
                                }
                                flush_pending = false;
                                segment_idx += 1;
                                last_segment_text.clear();
                                telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                                telemetry.on_started();
                            }
                            Ok(ElevenLabsParsedMessage::Error(message)) => {
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
                        let _ = ws_write.close().await;
                        return;
                    }
                    flush_pending = false;
                    segment_idx += 1;
                    last_segment_text.clear();
                    telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                    telemetry.on_started();
                }
            }
        }
    }
}

fn build_audio_message(
    data: &[u8],
    commit: bool,
    session: &ElevenLabsSessionConfig,
    previous_text: Option<&str>,
) -> String {
    let audio_base_64 = base64::engine::general_purpose::STANDARD.encode(data);
    let mut message = serde_json::json!({
        "message_type": "input_audio_chunk",
        "audio_base_64": audio_base_64,
        "sample_rate": session.sample_rate_hz,
        "commit": commit,
    });
    if let Some(previous_text) = previous_text {
        message["previous_text"] = serde_json::json!(previous_text);
    }
    message.to_string()
}

#[allow(clippy::too_many_arguments)] // justified: final assembly spans request state plus provider telemetry; keeping it local avoids shared mutable structs in the duplex loop
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
                    confidence: None,
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
enum ElevenLabsParsedMessage {
    SessionStarted,
    Partial {
        transcript: String,
    },
    Committed {
        transcript: String,
    },
    CommittedWithTimestamps {
        transcript: String,
        words: Vec<WordTimestamp>,
        language_code: Option<String>,
    },
    Error(String),
    Ignored,
}

fn parse_elevenlabs_message(text: &str) -> Result<ElevenLabsParsedMessage, AsrError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("failed to parse ElevenLabs message: {e}"),
        )
    })?;
    let message_type = value
        .get("message_type")
        .or_else(|| value.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match message_type {
        "session_started" => Ok(ElevenLabsParsedMessage::SessionStarted),
        "partial_transcript" => Ok(ElevenLabsParsedMessage::Partial {
            transcript: first_string(&value, &["text", "transcript"]).unwrap_or_default(),
        }),
        "committed_transcript" => Ok(ElevenLabsParsedMessage::Committed {
            transcript: first_string(&value, &["text", "transcript"]).unwrap_or_default(),
        }),
        "committed_transcript_with_timestamps" => {
            let (transcript, words, language_code) = parse_timestamped_commit(value)?;
            Ok(ElevenLabsParsedMessage::CommittedWithTimestamps {
                transcript,
                words,
                language_code,
            })
        }
        "error" | "auth_error" | "quota_exceeded" => Ok(ElevenLabsParsedMessage::Error(
            first_string(&value, &["message", "error", "detail"])
                .unwrap_or_else(|| "ElevenLabs stream error".into()),
        )),
        _ => Ok(ElevenLabsParsedMessage::Ignored),
    }
}

fn parse_timestamped_commit(
    value: serde_json::Value,
) -> Result<(String, Vec<WordTimestamp>, Option<String>), AsrError> {
    let message: ElevenLabsTimestampedMessage = serde_json::from_value(value).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("invalid ElevenLabs timestamped transcript: {e}"),
        )
    })?;
    let transcript = message.text.or(message.transcript).unwrap_or_else(|| {
        message
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    });
    let words = message
        .words
        .into_iter()
        .filter(|word| word.word_type.as_deref().unwrap_or("word") == "word")
        .map(|word| WordTimestamp {
            word: word.text,
            start_ms: seconds_to_ms(word.start),
            end_ms: seconds_to_ms(word.end),
            confidence: None,
        })
        .collect();
    Ok((transcript, words, message.language_code))
}

fn first_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(|v| v.as_str()).map(String::from))
}

fn seconds_to_ms(seconds: f64) -> u64 {
    if seconds <= 0.0 {
        0
    } else {
        (seconds * 1000.0).round() as u64
    }
}

#[derive(Debug, Deserialize)]
struct ElevenLabsTimestampedMessage {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    transcript: Option<String>,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    words: Vec<ElevenLabsWord>,
}

#[derive(Debug, Deserialize)]
struct ElevenLabsWord {
    text: String,
    start: f64,
    end: f64,
    #[serde(default, rename = "type")]
    word_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StreamingTranscribeRequest {
        StreamingTranscribeRequest {
            model: Some("elevenlabs/scribe_v2_realtime".into()),
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
            provider_options: serde_json::json!({
                "include_language_detection": true,
                "keyterms": ["runtime"],
                "enable_logging": false
            }),
        }
    }

    #[test]
    fn capabilities_are_realtime_only_and_explicit_about_diarization() {
        let caps = elevenlabs_capabilities();
        assert!(caps.streaming);
        assert!(!caps.batch);
        assert!(caps.word_timestamps);
        assert!(caps.hot_words);
        assert!(!caps.speaker_diarization);
        assert!(caps.provider_option_keys.contains(&"keyterms".to_string()));
        assert!(caps
            .endpointing_modes
            .contains(&EndpointingMode::AcousticSilence));
    }

    #[test]
    fn build_url_maps_language_detection_keyterms_and_vad() {
        let request = request();
        let session = ElevenLabsSessionConfig::from_request(&request).unwrap();
        let url = build_ws_url(
            "wss://api.elevenlabs.io/v1/speech-to-text/stream",
            "scribe_v2_realtime",
            &request,
            &session,
        )
        .unwrap();
        assert!(url.contains("model_id=scribe_v2_realtime"));
        assert!(url.contains("audio_format=pcm_16000"));
        assert!(url.contains("include_timestamps=true"));
        assert!(url.contains("include_language_detection=true"));
        assert!(url.contains("language_code=en"));
        assert!(url.contains("commit_strategy=vad"));
        assert!(url.contains("vad_silence_threshold_secs=0.75"));
        assert!(url.contains("keyterms=orchest"));
        assert!(url.contains("keyterms=runtime"));
        assert!(url.contains("enable_logging=false"));
    }

    #[test]
    fn session_rejects_unsupported_sample_rate() {
        let mut request = request();
        request.format = StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 12345,
            channels: 1,
        };
        let err = ElevenLabsSessionConfig::from_request(&request).unwrap_err();
        assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
    }

    #[test]
    fn audio_message_base64_encodes_chunk_and_commit() {
        let request = request();
        let session = ElevenLabsSessionConfig::from_request(&request).unwrap();
        let message = build_audio_message(&[1, 2, 3], true, &session, Some("hello"));
        let value: serde_json::Value = serde_json::from_str(&message).unwrap();
        assert_eq!(value["message_type"], "input_audio_chunk");
        assert_eq!(value["audio_base_64"], "AQID");
        assert_eq!(value["sample_rate"], 16000);
        assert_eq!(value["commit"], true);
        assert_eq!(value["previous_text"], "hello");
    }

    #[test]
    fn parse_partial_result() {
        let parsed =
            parse_elevenlabs_message(r#"{"message_type":"partial_transcript","text":"hello wor"}"#)
                .unwrap();
        match parsed {
            ElevenLabsParsedMessage::Partial { transcript } => assert_eq!(transcript, "hello wor"),
            _ => panic!("expected partial"),
        }
    }

    #[test]
    fn parse_timestamped_commit_uses_words() {
        let parsed = parse_elevenlabs_message(
            r#"{
                "message_type": "committed_transcript_with_timestamps",
                "text": "hello world",
                "language_code": "en",
                "words": [
                    {"text": "hello", "start": 0.0, "end": 0.5, "type": "word"},
                    {"text": " ", "start": 0.5, "end": 0.5, "type": "spacing"},
                    {"text": "world", "start": 0.5, "end": 1.0, "type": "word"}
                ]
            }"#,
        )
        .unwrap();
        match parsed {
            ElevenLabsParsedMessage::CommittedWithTimestamps {
                transcript,
                words,
                language_code,
            } => {
                assert_eq!(transcript, "hello world");
                assert_eq!(language_code.as_deref(), Some("en"));
                assert_eq!(words.len(), 2);
                assert_eq!(words[1].start_ms, 500);
            }
            _ => panic!("expected timestamped commit"),
        }
    }

    #[tokio::test]
    async fn transcribe_returns_unsupported_operation() {
        let adapter = ElevenLabsAsrAdapter::new(ElevenLabsAsrConfig::scribe_v2_realtime("key"));
        let err = adapter
            .transcribe(TranscribeRequest {
                model: Some("elevenlabs/scribe_v2_realtime".into()),
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
        assert_eq!(err.model.as_deref(), Some("elevenlabs/scribe_v2_realtime"));
    }

    #[tokio::test]
    async fn direct_stream_rejects_diarization() {
        let adapter = ElevenLabsAsrAdapter::new(ElevenLabsAsrConfig::scribe_v2_realtime("key"));
        let mut request = request();
        request.options.speaker_diarization = true;
        let err = match adapter.start_stream(request).await {
            Err(err) => err,
            Ok(_) => panic!("speaker diarization should be rejected before network connect"),
        };
        assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
        assert!(err.message.contains("speaker_diarization"));
    }

    #[tokio::test]
    async fn elevenlabs_rejects_insecure_ws_url() {
        let adapter = ElevenLabsAsrAdapter::new(ElevenLabsAsrConfig {
            model: "scribe_v2_realtime".into(),
            ws_url: "ws://api.elevenlabs.io/v1/speech-to-text/stream".into(),
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
