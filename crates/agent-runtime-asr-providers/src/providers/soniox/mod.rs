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
pub struct SonioxAsrConfig {
    /// Soniox realtime model identifier, for example "stt-rt-v5".
    pub model: String,
    /// WebSocket endpoint URL.
    pub ws_url: String,
    pub api_key: String,
}

impl SonioxAsrConfig {
    pub fn stt_rt_v5(api_key: impl Into<String>) -> Self {
        Self {
            model: "stt-rt-v5".into(),
            ws_url: "wss://stt-rt.soniox.com/transcribe-websocket".into(),
            api_key: api_key.into(),
        }
    }
}

pub struct SonioxAsrAdapter {
    config: SonioxAsrConfig,
}

impl SonioxAsrAdapter {
    pub fn new(config: SonioxAsrConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AsrProvider for SonioxAsrAdapter {
    fn provider_name(&self) -> &str {
        "soniox"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        soniox_capabilities()
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, _request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        Err(AsrError::unsupported_operation().with_model(format!("soniox/{}", self.config.model)))
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        if request.options.speaker_diarization {
            return Err(AsrError::new(
                AsrErrorCode::UnsupportedOption,
                "Soniox adapter does not support speaker_diarization yet",
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
        let model = format!("soniox/{}", self.config.model);
        let session = SonioxSessionConfig::from_request(&request)?;
        let config_message =
            build_config_message(&self.config.api_key, &self.config.model, &request, &session);

        let ws_request = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
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
                config_message,
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

fn soniox_capabilities() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![
            Language::new("*"),
            Language::new("multi"),
            Language::new("en"),
            Language::new("es"),
            Language::new("fr"),
            Language::new("de"),
            Language::new("ja"),
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
        code_switching: true,
        hot_words: true,
        context_prompt: true,
        provider_option_keys: vec![
            "language_hints",
            "language_hints_strict",
            "enable_language_identification",
            "enable_endpoint_detection",
            "max_endpoint_delay_ms",
            "context",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        max_duration_ms: None,
        default_flush_timeout_ms: Some(3000),
        source: CapabilitySource::Static,
        diagnostic_metadata: serde_json::json!({
            "provider": "soniox",
            "streaming_endpoint": "wss://stt-rt.soniox.com/transcribe-websocket",
            "transcribe": "unsupported_realtime_only"
        }),
    }
}

#[derive(Debug, Clone)]
struct SonioxSessionConfig {
    audio_format: serde_json::Value,
    enable_language_identification: bool,
    language_hints: Vec<String>,
    language_hints_strict: bool,
    enable_endpoint_detection: bool,
    max_endpoint_delay_ms: Option<u64>,
    context: Option<String>,
}

impl SonioxSessionConfig {
    fn from_request(request: &StreamingTranscribeRequest) -> Result<Self, AsrError> {
        let audio_format = match &request.format {
            StreamingAudioFormat::Pcm16 {
                sample_rate_hz,
                channels,
            } => serde_json::json!({
                "type": "pcm_s16le",
                "sample_rate": sample_rate_hz,
                "num_channels": channels,
            }),
            StreamingAudioFormat::Encoded { format } => {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedAudioFormat,
                    format!(
                        "Soniox adapter currently supports streaming PCM16 only; got {:?}",
                        format
                    ),
                ))
            }
        };

        let mut language_hints = Vec::new();
        if let Some(language) = &request.options.language {
            if language.as_str() != "*" && language.as_str() != "multi" {
                language_hints.push(language.as_str().to_string());
            }
        }
        if let Some(extra) = request.provider_options.get("language_hints") {
            if let Some(s) = extra.as_str() {
                language_hints.push(s.to_string());
            } else if let Some(values) = extra.as_array() {
                language_hints.extend(values.iter().filter_map(|v| v.as_str().map(String::from)));
            }
        }
        language_hints.extend(request.options.hot_words.iter().cloned());

        let enable_language_identification = request
            .provider_options
            .get("enable_language_identification")
            .and_then(|v| v.as_bool())
            .unwrap_or(request.options.code_switching || request.options.language.is_none());
        let language_hints_strict = request
            .provider_options
            .get("language_hints_strict")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut enable_endpoint_detection = request
            .provider_options
            .get("enable_endpoint_detection")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let mut max_endpoint_delay_ms = request
            .provider_options
            .get("max_endpoint_delay_ms")
            .and_then(|v| v.as_u64());

        if let Some(endpointing) = &request.options.endpointing {
            match endpointing.mode {
                EndpointingMode::ProviderDefault => {}
                EndpointingMode::ProviderDisabled => enable_endpoint_detection = false,
                EndpointingMode::AcousticSilence => {
                    enable_endpoint_detection = true;
                    if let Some(timeout) = endpointing.silence_timeout {
                        max_endpoint_delay_ms = Some(timeout.as_millis() as u64);
                    }
                }
                EndpointingMode::Semantic | EndpointingMode::NaturalSegmenting => {
                    return Err(AsrError::new(
                        AsrErrorCode::UnsupportedOption,
                        format!(
                            "Soniox adapter does not support {:?} endpointing",
                            endpointing.mode
                        ),
                    ))
                }
            }
        }

        let context = request
            .provider_options
            .get("context")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| request.options.context_prompt.clone());

        Ok(Self {
            audio_format,
            enable_language_identification,
            language_hints,
            language_hints_strict,
            enable_endpoint_detection,
            max_endpoint_delay_ms,
            context,
        })
    }
}

fn build_config_message(
    api_key: &str,
    model: &str,
    request: &StreamingTranscribeRequest,
    session: &SonioxSessionConfig,
) -> String {
    let mut value = serde_json::json!({
        "api_key": api_key,
        "model": model,
        "audio_format": session.audio_format,
        "enable_language_identification": session.enable_language_identification,
        "language_hints": session.language_hints,
        "language_hints_strict": session.language_hints_strict,
        "enable_endpoint_detection": session.enable_endpoint_detection,
    });

    if request.options.word_timestamps {
        value["enable_word_time_offsets"] = serde_json::json!(true);
    }
    if let Some(delay) = session.max_endpoint_delay_ms {
        value["max_endpoint_delay_ms"] = serde_json::json!(delay);
    }
    if let Some(context) = &session.context {
        value["context"] = serde_json::json!(context);
    }
    value.to_string()
}

fn extract_host(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split('/').next())
        .unwrap_or("stt-rt.soniox.com")
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // justified: single-function duplex WebSocket adapter; splitting would scatter the session state machine across helpers
async fn adapter_task(
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    config_message: String,
    mut audio_rx: mpsc::Receiver<AudioChunk>,
    event_tx: mpsc::Sender<AsrStreamEvent>,
    trace_id: String,
    model: String,
    flush_timeout: Duration,
    final_result_scope: FinalResultScope,
    language: Option<Language>,
) {
    let (mut ws_write, mut ws_read) = ws_stream.split();

    if ws_write
        .send(tungstenite::Message::Text(config_message))
        .await
        .is_err()
    {
        let _ = event_tx
            .send(AsrStreamEvent::Error {
                trace_id: trace_id.clone(),
                error: AsrError::new(
                    AsrErrorCode::ProviderStreamError,
                    "failed to send Soniox config",
                ),
                fatal: true,
            })
            .await;
        return;
    }

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
                                let message = if end_requested {
                                    tungstenite::Message::Binary(Vec::new())
                                } else {
                                    tungstenite::Message::Text(serde_json::json!({"type": "finalize"}).to_string())
                                };
                                if ws_write.send(message).await.is_err() {
                                    let _ = event_tx.send(AsrStreamEvent::Error {
                                        trace_id: trace_id.clone(),
                                        error: AsrError::new(AsrErrorCode::ProviderStreamError, "failed to finalize Soniox stream"),
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
                        match parse_soniox_message(&text) {
                            Ok(SonioxParsedMessage::Tokens { final_text, provisional_text, words, language, final_audio_ms, total_audio_ms, finished }) => {
                                if let Some(total_audio_ms) = total_audio_ms {
                                    audio_duration_ms = total_audio_ms;
                                }
                                if let Some(final_audio_ms) = final_audio_ms {
                                    audio_duration_ms = final_audio_ms;
                                }
                                if let Some(language) = language {
                                    telemetry.set_language(language);
                                }
                                if !final_text.is_empty() {
                                    last_segment_text.push_str(&final_text);
                                    segment_words.extend(words);
                                    let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                        trace_id: trace_id.clone(),
                                        segment_id: Some(format!("seg-{segment_idx}")),
                                        text: final_text,
                                        stability: TranscriptStability::Committed,
                                        update_kind: TranscriptUpdateKind::Append,
                                    }).await;
                                    telemetry.on_transcript_update(None, false);
                                }
                                let has_provisional = !provisional_text.is_empty();
                                if has_provisional {
                                    let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                        trace_id: trace_id.clone(),
                                        segment_id: Some(format!("seg-{segment_idx}")),
                                        text: provisional_text.clone(),
                                        stability: TranscriptStability::Provisional,
                                        update_kind: TranscriptUpdateKind::Snapshot,
                                    }).await;
                                    telemetry.on_transcript_update(None, false);
                                }

                                let can_finalize_after_flush = flush_pending && !has_provisional;
                                if finished || can_finalize_after_flush {
                                    let reason = if finished && !flush_pending {
                                        AsrFinalReason::ProviderEndpoint
                                    } else if end_requested {
                                        AsrFinalReason::CallerEnd
                                    } else {
                                        AsrFinalReason::CallerFlush
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
                                    if end_requested || finished {
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
                            Ok(SonioxParsedMessage::Error(message)) => {
                                let _ = event_tx.send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error: AsrError::new(AsrErrorCode::ProviderStreamError, message),
                                    fatal: true,
                                }).await;
                                return;
                            }
                            Ok(SonioxParsedMessage::Ignored) => {}
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
enum SonioxParsedMessage {
    Tokens {
        final_text: String,
        provisional_text: String,
        words: Vec<WordTimestamp>,
        language: Option<String>,
        final_audio_ms: Option<u64>,
        total_audio_ms: Option<u64>,
        finished: bool,
    },
    Error(String),
    Ignored,
}

fn parse_soniox_message(text: &str) -> Result<SonioxParsedMessage, AsrError> {
    let message: SonioxMessage = serde_json::from_str(text).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("failed to parse Soniox message: {e}"),
        )
    })?;
    if let Some(error_code) = message.error_code {
        return Ok(SonioxParsedMessage::Error(format!(
            "soniox error {error_code}: {}",
            message
                .error_message
                .unwrap_or_else(|| "stream error".into())
        )));
    }
    if message.tokens.is_empty() && !message.finished {
        return Ok(SonioxParsedMessage::Ignored);
    }

    let mut final_text = String::new();
    let mut provisional_text = String::new();
    let mut words = Vec::new();
    let mut language = None;
    for token in message.tokens {
        if token.is_final {
            final_text.push_str(&token.text);
            if token.text.trim().is_empty() {
                continue;
            }
            if language.is_none() {
                language = token.language.clone();
            }
            words.push(WordTimestamp {
                word: token.text,
                start_ms: token.start_ms.unwrap_or(0),
                end_ms: token.end_ms.unwrap_or(0),
                confidence: token.confidence,
            });
        } else {
            provisional_text.push_str(&token.text);
        }
    }

    Ok(SonioxParsedMessage::Tokens {
        final_text,
        provisional_text,
        words,
        language,
        final_audio_ms: message.final_audio_proc_ms,
        total_audio_ms: message.total_audio_proc_ms,
        finished: message.finished,
    })
}

#[derive(Debug, Deserialize)]
struct SonioxMessage {
    #[serde(default)]
    tokens: Vec<SonioxToken>,
    #[serde(default)]
    final_audio_proc_ms: Option<u64>,
    #[serde(default)]
    total_audio_proc_ms: Option<u64>,
    #[serde(default)]
    finished: bool,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SonioxToken {
    text: String,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    start_ms: Option<u64>,
    #[serde(default)]
    end_ms: Option<u64>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    language: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StreamingTranscribeRequest {
        StreamingTranscribeRequest {
            model: Some("soniox/stt-rt-v5".into()),
            format: StreamingAudioFormat::Pcm16 {
                sample_rate_hz: 16000,
                channels: 1,
            },
            timeline: AudioTimelineMode::ContinuousRealtime,
            options: TranscribeOptions {
                language: Some(Language::new("mixed:en,es")),
                code_switching: true,
                word_timestamps: true,
                hot_words: vec!["orchest".into()],
                context_prompt: Some("agent runtime vocabulary".into()),
                endpointing: Some(EndpointingOptions {
                    mode: EndpointingMode::AcousticSilence,
                    silence_timeout: Some(Duration::from_millis(750)),
                }),
                ..Default::default()
            },
            compatibility: CompatibilityPolicy::Strict,
            provider_options: serde_json::json!({
                "language_hints": ["en", "es"],
                "language_hints_strict": false
            }),
        }
    }

    #[test]
    fn capabilities_advertise_multilingual_code_switching() {
        let caps = soniox_capabilities();
        assert!(caps.streaming);
        assert!(caps.code_switching);
        assert!(caps.hot_words);
        assert!(caps.context_prompt);
        assert!(caps
            .provider_option_keys
            .contains(&"language_hints".to_string()));
        assert!(caps.languages.contains(&Language::new("*")));
    }

    #[test]
    fn config_message_maps_code_switching_language_hints_and_endpointing() {
        let request = request();
        let session = SonioxSessionConfig::from_request(&request).unwrap();
        let message = build_config_message("key", "stt-rt-v5", &request, &session);
        let value: serde_json::Value = serde_json::from_str(&message).unwrap();
        assert_eq!(value["api_key"], "key");
        assert_eq!(value["model"], "stt-rt-v5");
        assert_eq!(value["audio_format"]["type"], "pcm_s16le");
        assert_eq!(value["audio_format"]["sample_rate"], 16000);
        assert_eq!(value["enable_language_identification"], true);
        assert_eq!(value["enable_word_time_offsets"], true);
        assert_eq!(value["enable_endpoint_detection"], true);
        assert_eq!(value["max_endpoint_delay_ms"], 750);
        assert_eq!(value["context"], "agent runtime vocabulary");
        assert!(value["language_hints"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("en")));
        assert!(value["language_hints"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("es")));
        assert!(value["language_hints"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("orchest")));
    }

    #[test]
    fn parse_partial_and_final_tokens() {
        let parsed = parse_soniox_message(
            r#"{
                "tokens": [
                    {"text": "hello", "is_final": true, "start_ms": 0, "end_ms": 500, "confidence": 0.91, "language": "en"},
                    {"text": " mundo", "is_final": false, "language": "es"}
                ],
                "final_audio_proc_ms": 500,
                "total_audio_proc_ms": 900
            }"#,
        )
        .unwrap();
        match parsed {
            SonioxParsedMessage::Tokens {
                final_text,
                provisional_text,
                words,
                language,
                final_audio_ms,
                total_audio_ms,
                finished,
            } => {
                assert_eq!(final_text, "hello");
                assert_eq!(provisional_text, " mundo");
                assert_eq!(words.len(), 1);
                assert_eq!(words[0].confidence, Some(0.91));
                assert_eq!(language.as_deref(), Some("en"));
                assert_eq!(final_audio_ms, Some(500));
                assert_eq!(total_audio_ms, Some(900));
                assert!(!finished);
            }
            _ => panic!("expected tokens"),
        }
    }

    #[test]
    fn parse_error_message() {
        let parsed = parse_soniox_message(
            r#"{"error_code":"bad_request","error_message":"invalid config"}"#,
        )
        .unwrap();
        match parsed {
            SonioxParsedMessage::Error(message) => {
                assert!(message.contains("bad_request"));
                assert!(message.contains("invalid config"));
            }
            _ => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn transcribe_returns_unsupported_operation() {
        let adapter = SonioxAsrAdapter::new(SonioxAsrConfig::stt_rt_v5("key"));
        let err = adapter
            .transcribe(TranscribeRequest {
                model: Some("soniox/stt-rt-v5".into()),
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
        assert_eq!(err.model.as_deref(), Some("soniox/stt-rt-v5"));
    }

    #[tokio::test]
    async fn soniox_rejects_insecure_ws_url() {
        let adapter = SonioxAsrAdapter::new(SonioxAsrConfig {
            model: "stt-rt-v5".into(),
            ws_url: "ws://stt-rt.soniox.com/transcribe-websocket".into(),
            api_key: "key".into(),
        });
        let err = match adapter.start_stream(request()).await {
            Err(err) => err,
            Ok(_) => panic!("insecure WebSocket URL should be rejected before network connect"),
        };
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("wss://"));
    }

    #[tokio::test]
    async fn soniox_rejects_speaker_diarization() {
        let adapter = SonioxAsrAdapter::new(SonioxAsrConfig::stt_rt_v5("key"));
        let mut request = request();
        request.options.speaker_diarization = true;
        let err = match adapter.start_stream(request).await {
            Err(err) => err,
            Ok(_) => panic!("speaker diarization should be rejected before network connect"),
        };
        assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
        assert!(err.message.contains("speaker_diarization"));
    }
}
