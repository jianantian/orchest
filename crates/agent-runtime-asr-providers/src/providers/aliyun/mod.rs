use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tracing::Instrument;

use crate::error::{AsrError, AsrErrorCode};
use crate::observability::{self, AsrTelemetryBuilder};
use crate::streaming::{AsrAudioSink, AsrEventStream, AsrStream};
use crate::traits::AsrProvider;
use crate::types::*;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AliyunAsrConfig {
    pub model: String,
    pub api_key: String,
    pub ws_url: String,
}

impl AliyunAsrConfig {
    pub fn fun_asr_realtime(api_key: String) -> Self {
        Self {
            model: "fun-asr-realtime".into(),
            api_key,
            ws_url: "wss://dashscope.aliyuncs.com/api-ws/v1/inference/".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliyunProtocol {
    FunAsr,
    QwenAsr,
}

#[derive(Serialize)]
struct DashScopeMessage {
    header: DashScopeClientHeader,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct DashScopeClientHeader {
    action: String,
    task_id: String,
    streaming: String,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeServerEvent {
    pub header: DashScopeServerHeader,
    pub payload: Option<DashScopeServerPayload>,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeServerHeader {
    pub task_id: String,
    pub event: String,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeServerPayload {
    pub output: Option<DashScopeOutput>,
    pub usage: Option<DashScopeUsage>,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeOutput {
    pub sentence: Option<DashScopeSentence>,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeSentence {
    pub text: String,
    #[serde(default)]
    pub begin_time: Option<i64>,
    #[serde(default)]
    pub end_time: Option<i64>,
    #[serde(default)]
    pub sentence_end: bool,
    #[serde(default)]
    pub words: Option<Vec<DashScopeWord>>,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeWord {
    pub text: String,
    pub begin_time: i64,
    pub end_time: i64,
    #[serde(default)]
    pub punctuation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DashScopeUsage {
    pub duration: Option<f64>,
}

// ---------------------------------------------------------------------------
// Message builders
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)] // justified: WebSocket message builder mirrors the provider wire protocol fields 1:1
pub fn build_run_task(
    task_id: &str,
    model: &str,
    sample_rate: u32,
    format: &str,
    max_sentence_silence: Option<u64>,
    provider_options: &serde_json::Value,
) -> Result<String, AsrError> {
    let mut parameters = serde_json::json!({
        "sample_rate": sample_rate,
        "format": format,
    });

    if let Some(silence) = max_sentence_silence {
        parameters["max_sentence_silence"] = serde_json::json!(silence);
    }

    if let Some(obj) = provider_options.as_object() {
        for (k, v) in obj {
            parameters[k] = v.clone();
        }
    }

    let msg = DashScopeMessage {
        header: DashScopeClientHeader {
            action: "run-task".into(),
            task_id: task_id.into(),
            streaming: "duplex".into(),
        },
        payload: serde_json::json!({
            "task_group": "audio",
            "task": "asr",
            "function": "recognition",
            "model": model,
            "parameters": parameters,
            "input": {},
        }),
    };
    serde_json::to_string(&msg).map_err(|e| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("failed to serialize aliyun task message: {e}"),
        )
    })
}

pub fn build_finish_task(task_id: &str) -> Result<String, AsrError> {
    let msg = DashScopeMessage {
        header: DashScopeClientHeader {
            action: "finish-task".into(),
            task_id: task_id.into(),
            streaming: "duplex".into(),
        },
        payload: serde_json::json!({
            "input": {},
        }),
    };
    serde_json::to_string(&msg).map_err(|e| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("failed to serialize aliyun task message: {e}"),
        )
    })
}

pub fn parse_server_event(text: &str) -> Result<DashScopeServerEvent, AsrError> {
    serde_json::from_str(text).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("failed to parse DashScope event: {e}"),
        )
    })
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct AliyunAsrAdapter {
    config: AliyunAsrConfig,
}

impl AliyunAsrAdapter {
    pub fn new(config: AliyunAsrConfig) -> Self {
        Self { config }
    }

    fn protocol(&self) -> AliyunProtocol {
        if self.config.model.starts_with("qwen") {
            AliyunProtocol::QwenAsr
        } else {
            AliyunProtocol::FunAsr
        }
    }

    fn fun_asr_capabilities(&self) -> AsrModelCapabilities {
        AsrModelCapabilities {
            languages: vec![
                Language::new("zh-CN"),
                Language::new("en"),
                Language::new("ja"),
            ],
            streaming: true,
            batch: false,
            streaming_inputs: vec![
                AudioInputCapability {
                    format: AudioFormat::Pcm,
                    sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
                    channels: ChannelSupport::Exact(vec![1]),
                    max_duration_ms: None,
                    max_bytes: None,
                },
                AudioInputCapability {
                    format: AudioFormat::Wav,
                    sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
                    channels: ChannelSupport::Exact(vec![1]),
                    max_duration_ms: None,
                    max_bytes: None,
                },
                AudioInputCapability {
                    format: AudioFormat::Mp3,
                    sample_rates_hz: SampleRateSupport::Any,
                    channels: ChannelSupport::Exact(vec![1]),
                    max_duration_ms: None,
                    max_bytes: None,
                },
                AudioInputCapability {
                    format: AudioFormat::Opus,
                    sample_rates_hz: SampleRateSupport::Any,
                    channels: ChannelSupport::Exact(vec![1]),
                    max_duration_ms: None,
                    max_bytes: None,
                },
            ],
            batch_inputs: vec![],
            audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
            interim_results: true,
            endpointing_modes: vec![EndpointingMode::AcousticSilence],
            segment_flush: true,
            multi_segment_streaming: true,
            connection_reuse: ConnectionReuse::ReusableAfterProviderTaskFinished,
            word_timestamps: true,
            speaker_diarization: false,
            confidence: false,
            code_switching: true,
            hot_words: true,
            context_prompt: false,
            provider_option_keys: vec![
                "max_sentence_silence".into(),
                "semantic_punctuation_enabled".into(),
                "vocabulary_id".into(),
            ],
            max_duration_ms: None,
            default_flush_timeout_ms: Some(3000),
            source: CapabilitySource::Static,
            diagnostic_metadata: serde_json::Value::Null,
        }
    }
}

#[async_trait]
impl AsrProvider for AliyunAsrAdapter {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        self.fun_asr_capabilities()
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, _request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        Err(AsrError::unsupported_operation())
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        if self.protocol() == AliyunProtocol::QwenAsr {
            return Err(AsrError::new(
                AsrErrorCode::UnsupportedOperation,
                "qwen-asr is not yet implemented",
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
        let model = format!("aliyun/{}", self.config.model);
        let task_id = uuid::Uuid::new_v4().simple().to_string();

        let ws_request = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("Authorization", format!("bearer {}", self.config.api_key))
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

        let (sample_rate, audio_format) = match &request.format {
            StreamingAudioFormat::Pcm16 { sample_rate_hz, .. } => (*sample_rate_hz, "pcm"),
            StreamingAudioFormat::Encoded { format } => {
                let fmt = match format {
                    AudioFormat::Wav => "wav",
                    AudioFormat::Mp3 => "mp3",
                    AudioFormat::Opus => "opus",
                    _ => "pcm",
                };
                (16000, fmt)
            }
        };

        let max_sentence_silence = request
            .options
            .endpointing
            .as_ref()
            .and_then(|e| e.silence_timeout)
            .map(|d| d.as_millis() as u64);

        let run_task_msg = build_run_task(
            &task_id,
            &self.config.model,
            sample_rate,
            audio_format,
            max_sentence_silence,
            &request.provider_options,
        )?;

        let flush_timeout = request
            .options
            .flush_timeout
            .unwrap_or(Duration::from_secs(3));
        let final_result_scope = request.options.final_result_scope.clone();

        let (audio_tx, audio_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(64);

        let adapter_trace_id = trace_id.clone();
        let adapter_model = model.clone();

        event_tx
            .send(AsrStreamEvent::RouteSelected {
                trace_id: trace_id.clone(),
                model: model.clone(),
            })
            .await
            .ok();

        let task_params = TaskParams {
            aliyun_model: self.config.model.clone(),
            sample_rate,
            audio_format: audio_format.to_string(),
            max_sentence_silence,
            provider_options: request.provider_options.clone(),
        };

        tokio::spawn(async move {
            adapter_task(
                ws_stream,
                run_task_msg,
                task_id,
                audio_rx,
                event_tx,
                adapter_trace_id,
                adapter_model,
                flush_timeout,
                final_result_scope,
                task_params,
            )
            .await;
        });

        Ok(AsrStream::new(
            AsrAudioSink::new(audio_tx),
            AsrEventStream::new(event_rx),
        ))
    }
}

fn extract_host(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split('/').next())
        .unwrap_or("dashscope.aliyuncs.com")
}

// ---------------------------------------------------------------------------
// Adapter task
// ---------------------------------------------------------------------------

struct TaskParams {
    aliyun_model: String,
    sample_rate: u32,
    audio_format: String,
    max_sentence_silence: Option<u64>,
    provider_options: serde_json::Value,
}

impl TaskParams {
    fn build_run_task(&self, task_id: &str) -> Result<String, AsrError> {
        build_run_task(
            task_id,
            &self.aliyun_model,
            self.sample_rate,
            &self.audio_format,
            self.max_sentence_silence,
            &self.provider_options,
        )
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // justified: single-function duplex WebSocket adapter; splitting would scatter the session state machine across helpers
async fn adapter_task(
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    run_task_msg: String,
    initial_task_id: String,
    mut audio_rx: mpsc::Receiver<AudioChunk>,
    event_tx: mpsc::Sender<AsrStreamEvent>,
    trace_id: String,
    model: String,
    flush_timeout: Duration,
    final_result_scope: FinalResultScope,
    task_params: TaskParams,
) {
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Send run-task
    if ws_write
        .send(tungstenite::Message::Text(run_task_msg))
        .await
        .is_err()
    {
        let _ = event_tx
            .send(AsrStreamEvent::Error {
                trace_id: trace_id.clone(),
                error: AsrError::new(AsrErrorCode::ProviderStreamError, "failed to send run-task"),
                fatal: true,
            })
            .await;
        return;
    }

    // Wait for task-started
    loop {
        match ws_read.next().await {
            Some(Ok(tungstenite::Message::Text(text))) => {
                if let Ok(event) = parse_server_event(&text) {
                    match event.header.event.as_str() {
                        "task-started" => break,
                        "task-failed" => {
                            let msg = event
                                .header
                                .error_message
                                .unwrap_or_else(|| "task-failed during start".into());
                            let _ = event_tx
                                .send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error: AsrError::new(AsrErrorCode::ProviderTaskFailed, msg),
                                    fatal: true,
                                })
                                .await;
                            return;
                        }
                        _ => continue,
                    }
                }
            }
            Some(Err(e)) => {
                let _ = event_tx
                    .send(AsrStreamEvent::Error {
                        trace_id: trace_id.clone(),
                        error: AsrError::new(
                            AsrErrorCode::ProviderStreamError,
                            format!("WebSocket error waiting for task-started: {e}"),
                        ),
                        fatal: true,
                    })
                    .await;
                return;
            }
            None => {
                let _ = event_tx
                    .send(AsrStreamEvent::Error {
                        trace_id: trace_id.clone(),
                        error: AsrError::new(
                            AsrErrorCode::ProviderStreamError,
                            "WebSocket closed before task-started",
                        ),
                        fatal: true,
                    })
                    .await;
                return;
            }
            _ => continue,
        }
    }

    let _ = event_tx
        .send(AsrStreamEvent::Started {
            trace_id: trace_id.clone(),
            model: model.clone(),
        })
        .await;

    let mut telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
    telemetry.on_started();

    let mut flush_pending = false;
    let mut flush_started: Option<std::time::Instant> = None;
    let mut end_requested = false;
    let mut accumulated_text = String::new();
    let mut segment_idx = 0u32;
    let mut last_segment_text = String::new();
    let mut audio_duration_ms: u64 = 0;
    let mut _current_task_id = initial_task_id;

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
                        match chunk.boundary {
                            AudioChunkBoundary::None => {
                                if !chunk.data.is_empty() && ws_write.send(tungstenite::Message::Binary(chunk.data.to_vec())).await.is_err() {
                                    let _ = event_tx.send(AsrStreamEvent::Error {
                                        trace_id: trace_id.clone(),
                                        error: AsrError::new(AsrErrorCode::ProviderStreamError, "WebSocket send failed"),
                                        fatal: true,
                                    }).await;
                                    return;
                                }
                            }
                            AudioChunkBoundary::Flush | AudioChunkBoundary::End => {
                                if !chunk.data.is_empty() {
                                    let _ = ws_write.send(tungstenite::Message::Binary(chunk.data.to_vec())).await;
                                }
                                let finish_msg = match build_finish_task(&_current_task_id) {
                                    Ok(msg) => msg,
                                    Err(error) => {
                                        let _ = event_tx.send(AsrStreamEvent::Error {
                                            trace_id: trace_id.clone(),
                                            error,
                                            fatal: true,
                                        }).await;
                                        return;
                                    }
                                };
                                if ws_write.send(tungstenite::Message::Text(finish_msg)).await.is_err() {
                                    let _ = event_tx.send(AsrStreamEvent::Error {
                                        trace_id: trace_id.clone(),
                                        error: AsrError::new(AsrErrorCode::ProviderStreamError, "failed to send finish-task"),
                                        fatal: true,
                                    }).await;
                                    return;
                                }
                                flush_pending = true;
                                flush_started = Some(std::time::Instant::now());
                                if matches!(chunk.boundary, AudioChunkBoundary::End) {
                                    end_requested = true;
                                }
                            }
                        }
                    }
                    None => {
                        if !flush_pending {
                            let _ = event_tx.send(AsrStreamEvent::Error {
                                trace_id: trace_id.clone(),
                                error: AsrError::new(AsrErrorCode::Cancelled, "audio sink dropped"),
                                fatal: true,
                            }).await;
                        }
                        return;
                    }
                }
            }
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        if let Ok(event) = parse_server_event(&text) {
                            match event.header.event.as_str() {
                                "result-generated" => {
                                    if let Some(ref payload) = event.payload {
                                        if let Some(ref output) = payload.output {
                                            if let Some(ref sentence) = output.sentence {
                                                let stability = if sentence.sentence_end {
                                                    TranscriptStability::Committed
                                                } else {
                                                    TranscriptStability::Provisional
                                                };
                                                let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                                    trace_id: trace_id.clone(),
                                                    segment_id: Some(format!("seg-{segment_idx}")),
                                                    text: sentence.text.clone(),
                                                    stability,
                                                    update_kind: TranscriptUpdateKind::Snapshot,
                                                }).await;
                                                telemetry.on_transcript_update(None, false);

                                                if sentence.sentence_end {
                                                    last_segment_text = sentence.text.clone();
                                                }
                                            }
                                        }
                                        if let Some(ref usage) = payload.usage {
                                            if let Some(dur) = usage.duration {
                                                audio_duration_ms = (dur * 1000.0) as u64;
                                            }
                                        }
                                    }
                                }
                                "task-finished" => {
                                    #[allow(unused_assignments)]
                                    { flush_pending = false; }

                                    let text = match final_result_scope {
                                        FinalResultScope::Stream => {
                                            if !accumulated_text.is_empty() && !last_segment_text.is_empty() {
                                                accumulated_text.push(' ');
                                            }
                                            accumulated_text.push_str(&last_segment_text);
                                            accumulated_text.clone()
                                        }
                                        FinalResultScope::Segment => last_segment_text.clone(),
                                    };

                                    let reason = if end_requested {
                                        AsrFinalReason::CallerEnd
                                    } else {
                                        AsrFinalReason::CallerFlush
                                    };

                                    telemetry.set_audio_duration_ms(audio_duration_ms);
                                    let telem = telemetry.build();
                                    observability::record_final_latency(&model, telem.latency_final_ms);
                                    observability::record_audio_duration(&model, audio_duration_ms);

                                    let _ = event_tx.send(AsrStreamEvent::AsrFinal {
                                        final_output: Box::new(AsrFinalOutput {
                                            trace_id: trace_id.clone(),
                                            segment_id: Some(format!("seg-{segment_idx}")),
                                            reason,
                                            result: TranscribeResult {
                                                text,
                                                language: Some(Language::new("zh-CN")),
                                                confidence: None,
                                                words: vec![],
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
                                    }).await;

                                    if end_requested {
                                        let _ = ws_write.close().await;
                                        return;
                                    }

                                    // Connection reuse: start new task
                                    segment_idx += 1;
                                    _current_task_id = uuid::Uuid::new_v4().simple().to_string();
                                    last_segment_text.clear();
                                    flush_pending = false;

                                    let new_run_msg = match task_params.build_run_task(&_current_task_id) {
                                        Ok(msg) => msg,
                                        Err(error) => {
                                            let _ = event_tx.send(AsrStreamEvent::Error {
                                                trace_id: trace_id.clone(),
                                                error,
                                                fatal: true,
                                            }).await;
                                            return;
                                        }
                                    };
                                    if ws_write.send(tungstenite::Message::Text(new_run_msg)).await.is_err() {
                                        let _ = event_tx.send(AsrStreamEvent::Error {
                                            trace_id: trace_id.clone(),
                                            error: AsrError::new(AsrErrorCode::ProviderStreamError, "failed to send run-task for new segment"),
                                            fatal: true,
                                        }).await;
                                        return;
                                    }

                                    // Wait for task-started on the reused connection
                                    let mut reuse_ok = false;
                                    while let Some(msg) = ws_read.next().await {
                                        if let Ok(tungstenite::Message::Text(text)) = msg {
                                            if let Ok(ev) = parse_server_event(&text) {
                                                match ev.header.event.as_str() {
                                                    "task-started" => { reuse_ok = true; break; }
                                                    "task-failed" => {
                                                        let m = ev.header.error_message.unwrap_or_else(|| "task-failed during reuse".into());
                                                        let _ = event_tx.send(AsrStreamEvent::Error {
                                                            trace_id: trace_id.clone(),
                                                            error: AsrError::new(AsrErrorCode::ProviderTaskFailed, m),
                                                            fatal: true,
                                                        }).await;
                                                        return;
                                                    }
                                                    _ => continue,
                                                }
                                            }
                                        }
                                    }
                                    if !reuse_ok {
                                        let _ = event_tx.send(AsrStreamEvent::Error {
                                            trace_id: trace_id.clone(),
                                            error: AsrError::new(AsrErrorCode::ProviderStreamError, "WebSocket closed during connection reuse"),
                                            fatal: true,
                                        }).await;
                                        return;
                                    }

                                    telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                                    telemetry.on_started();
                                }
                                "task-failed" => {
                                    let msg = event.header.error_message.unwrap_or_else(|| "task-failed".into());
                                    let _ = event_tx.send(AsrStreamEvent::Error {
                                        trace_id: trace_id.clone(),
                                        error: AsrError::new(AsrErrorCode::ProviderTaskFailed, msg),
                                        fatal: true,
                                    }).await;
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => {
                        if flush_pending {
                            telemetry.set_audio_duration_ms(audio_duration_ms);
                            emit_timeout_final(
                                &event_tx, &trace_id, segment_idx,
                                &last_segment_text, &mut accumulated_text,
                                &final_result_scope, audio_duration_ms, telemetry,
                            ).await;
                        }
                        return;
                    }
                    Some(Err(e)) => {
                        let _ = event_tx.send(AsrStreamEvent::Error {
                            trace_id: trace_id.clone(),
                            error: AsrError::new(
                                AsrErrorCode::ProviderStreamError,
                                format!("WebSocket error: {e}"),
                            ),
                            fatal: true,
                        }).await;
                        return;
                    }
                    _ => {}
                }
            }
            _ = flush_timeout_fut => {
                if flush_pending {
                    telemetry.set_audio_duration_ms(audio_duration_ms);
                    emit_timeout_final(
                        &event_tx, &trace_id, segment_idx,
                        &last_segment_text, &mut accumulated_text,
                        &final_result_scope, audio_duration_ms, telemetry,
                    ).await;

                    // After timeout, the previous task may still be in-flight on
                    // the server side — connection reuse is unsafe. Close and stop.
                    let _ = ws_write.close().await;
                    if !end_requested {
                        let _ = event_tx.send(AsrStreamEvent::Error {
                            trace_id: trace_id.clone(),
                            error: AsrError::new(AsrErrorCode::ProviderStreamError, "connection closed after flush timeout; cannot reuse"),
                            fatal: true,
                        }).await;
                    }
                    return;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // justified: timeout finalization closes over all active channel senders; extracting a context struct would add boilerplate without clarity
async fn emit_timeout_final(
    event_tx: &mpsc::Sender<AsrStreamEvent>,
    trace_id: &str,
    segment_idx: u32,
    last_segment_text: &str,
    accumulated_text: &mut String,
    final_result_scope: &FinalResultScope,
    audio_duration_ms: u64,
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

    let _ = event_tx
        .send(AsrStreamEvent::AsrFinal {
            final_output: Box::new(AsrFinalOutput {
                trace_id: trace_id.to_string(),
                segment_id: Some(format!("seg-{segment_idx}")),
                reason: AsrFinalReason::Timeout,
                result: TranscribeResult {
                    text,
                    language: Some(Language::new("zh-CN")),
                    confidence: None,
                    words: vec![],
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_task_json_structure() {
        let json = build_run_task(
            "abc123",
            "fun-asr-realtime",
            16000,
            "pcm",
            Some(800),
            &serde_json::Value::Null,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["header"]["action"], "run-task");
        assert_eq!(parsed["header"]["task_id"], "abc123");
        assert_eq!(parsed["header"]["streaming"], "duplex");
        assert_eq!(parsed["payload"]["model"], "fun-asr-realtime");
        assert_eq!(parsed["payload"]["parameters"]["sample_rate"], 16000);
        assert_eq!(parsed["payload"]["parameters"]["format"], "pcm");
        assert_eq!(parsed["payload"]["parameters"]["max_sentence_silence"], 800);
    }

    #[test]
    fn finish_task_json_structure() {
        let json = build_finish_task("task-xyz").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["header"]["action"], "finish-task");
        assert_eq!(parsed["header"]["task_id"], "task-xyz");
    }

    #[test]
    fn parse_task_started() {
        let json = r#"{"header":{"task_id":"t1","event":"task-started"},"payload":{}}"#;
        let event = parse_server_event(json).unwrap();
        assert_eq!(event.header.event, "task-started");
        assert_eq!(event.header.task_id, "t1");
    }

    #[test]
    fn parse_result_generated() {
        let json = r#"{
            "header":{"task_id":"t1","event":"result-generated"},
            "payload":{
                "output":{
                    "sentence":{
                        "text":"你好世界",
                        "begin_time":100,
                        "end_time":2000,
                        "sentence_end":true,
                        "words":[
                            {"text":"你","begin_time":100,"end_time":500},
                            {"text":"好","begin_time":500,"end_time":900},
                            {"text":"世","begin_time":900,"end_time":1400},
                            {"text":"界","begin_time":1400,"end_time":2000}
                        ]
                    }
                }
            }
        }"#;
        let event = parse_server_event(json).unwrap();
        assert_eq!(event.header.event, "result-generated");
        let sentence = event.payload.unwrap().output.unwrap().sentence.unwrap();
        assert_eq!(sentence.text, "你好世界");
        assert!(sentence.sentence_end);
        assert_eq!(sentence.words.as_ref().unwrap().len(), 4);
        assert_eq!(sentence.words.as_ref().unwrap()[0].text, "你");
    }

    #[test]
    fn parse_result_partial() {
        let json = r#"{
            "header":{"task_id":"t1","event":"result-generated"},
            "payload":{"output":{"sentence":{"text":"你好","sentence_end":false}}}
        }"#;
        let event = parse_server_event(json).unwrap();
        let sentence = event.payload.unwrap().output.unwrap().sentence.unwrap();
        assert!(!sentence.sentence_end);
    }

    #[test]
    fn parse_task_finished_with_usage() {
        let json = r#"{
            "header":{"task_id":"t1","event":"task-finished"},
            "payload":{"usage":{"duration":10.5}}
        }"#;
        let event = parse_server_event(json).unwrap();
        assert_eq!(event.header.event, "task-finished");
        let usage = event.payload.unwrap().usage.unwrap();
        assert!((usage.duration.unwrap() - 10.5).abs() < 0.01);
    }

    #[test]
    fn parse_task_failed() {
        let json = r#"{
            "header":{"task_id":"t1","event":"task-failed","error_code":"InvalidRequest","error_message":"bad format"},
            "payload":{}
        }"#;
        let event = parse_server_event(json).unwrap();
        assert_eq!(event.header.event, "task-failed");
        assert_eq!(event.header.error_code.as_deref(), Some("InvalidRequest"));
        assert_eq!(event.header.error_message.as_deref(), Some("bad format"));
    }

    #[test]
    fn endpointing_mapping() {
        let json = build_run_task(
            "t1",
            "fun-asr-realtime",
            16000,
            "pcm",
            Some(500),
            &serde_json::Value::Null,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["payload"]["parameters"]["max_sentence_silence"], 500);
    }

    #[test]
    fn protocol_detection() {
        let fun = AliyunAsrAdapter::new(AliyunAsrConfig {
            model: "fun-asr-realtime".into(),
            api_key: "key".into(),
            ws_url: "ws://test".into(),
        });
        assert_eq!(fun.protocol(), AliyunProtocol::FunAsr);

        let qwen = AliyunAsrAdapter::new(AliyunAsrConfig {
            model: "qwen3-asr-flash-realtime".into(),
            api_key: "key".into(),
            ws_url: "ws://test".into(),
        });
        assert_eq!(qwen.protocol(), AliyunProtocol::QwenAsr);
    }

    #[test]
    fn run_task_with_provider_options() {
        let opts = serde_json::json!({"vocabulary_id": "vocab-123"});
        let json = build_run_task("t1", "fun-asr-realtime", 16000, "pcm", None, &opts).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["payload"]["parameters"]["vocabulary_id"],
            "vocab-123"
        );
    }

    #[test]
    fn host_extraction() {
        assert_eq!(
            extract_host("wss://dashscope.aliyuncs.com/api-ws/v1/inference/"),
            "dashscope.aliyuncs.com"
        );
    }

    #[tokio::test]
    async fn aliyun_rejects_insecure_ws_url() {
        let adapter = AliyunAsrAdapter::new(AliyunAsrConfig {
            model: "fun-asr-realtime".into(),
            api_key: "key".into(),
            ws_url: "ws://example.invalid/api-ws/v1/inference/".into(),
        });
        let request = StreamingTranscribeRequest {
            model: Some("aliyun/fun-asr-realtime".into()),
            format: StreamingAudioFormat::Pcm16 {
                sample_rate_hz: 16000,
                channels: 1,
            },
            timeline: AudioTimelineMode::ContinuousRealtime,
            options: TranscribeOptions::default(),
            compatibility: CompatibilityPolicy::Coerce,
            provider_options: serde_json::Value::Null,
        };

        let err = match adapter.start_stream(request).await {
            Ok(_) => panic!("insecure WebSocket URL should be rejected before connect"),
            Err(err) => err,
        };

        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("wss://"));
    }
}
