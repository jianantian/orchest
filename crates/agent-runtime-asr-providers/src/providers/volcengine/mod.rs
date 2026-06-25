pub mod protocol;
pub mod realtime;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tracing::Instrument;

use crate::error::{AsrError, AsrErrorCode};
use crate::observability::{self, AsrTelemetryBuilder};
use crate::streaming::{AsrAudioSink, AsrEventStream, AsrStream};
use crate::traits::AsrProvider;
use crate::types::*;

use protocol::*;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct VolcengineAsrConfig {
    /// Model identifier: "bigasr" (Doubao ASR 1.0) or "seedasr" (Doubao ASR 2.0).
    /// Maps to the `X-Api-Resource-Id` header; see `resource_id`.
    pub model: String,
    /// WebSocket endpoint URL, e.g. "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async".
    /// Selects the streaming protocol variant (bigmodel / bigmodel_nostream / bigmodel_async).
    pub ws_url: String,
    pub api_key: String,
    pub access_key: Option<String>,
    /// Provider-side resource identifier (X-Api-Resource-Id).
    /// Normally derived from `model`; can also encode the billing tier
    /// (e.g. "volc.bigasr.sauc.concurrent" for concurrent billing).
    pub resource_id: String,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct VolcengineAsrAdapter {
    config: VolcengineAsrConfig,
}

impl VolcengineAsrAdapter {
    pub fn new(config: VolcengineAsrConfig) -> Self {
        Self { config }
    }

    fn build_client_payload(&self, request: &StreamingTranscribeRequest) -> serde_json::Value {
        let (rate, channel) = match &request.format {
            StreamingAudioFormat::Pcm16 {
                sample_rate_hz,
                channels,
            } => (*sample_rate_hz, *channels),
            StreamingAudioFormat::Encoded { .. } => (16000, 1),
        };

        let mut req_obj = serde_json::json!({
            "model_name": "bigmodel",
            "show_utterances": true,
            "result_type": "single",
            "enable_itn": request.options.punctuate,
            "enable_punc": request.options.punctuate,
        });

        if let Some(ref endpointing) = request.options.endpointing {
            if let Some(timeout) = endpointing.silence_timeout {
                req_obj["end_window_size"] = serde_json::json!(timeout.as_millis() as u64);
            }
        }

        if !request.options.hot_words.is_empty() {
            let hotwords: Vec<serde_json::Value> = request
                .options
                .hot_words
                .iter()
                .map(|w| serde_json::json!({"word": w}))
                .collect();
            let context_str = serde_json::json!({"hotwords": hotwords}).to_string();
            req_obj["corpus"] = serde_json::json!({"context": context_str});
        } else if let Some(ref ctx) = request.options.context_prompt {
            req_obj["corpus"] = serde_json::json!({"context": ctx});
        }

        if let Some(po) = request.provider_options.as_object() {
            for (k, v) in po.iter() {
                req_obj[k] = v.clone();
            }
        }

        serde_json::json!({
            "user": {"uid": "orchest-sdk"},
            "audio": {
                "format": "pcm",
                "rate": rate,
                "bits": 16,
                "channel": channel,
            },
            "request": req_obj,
        })
    }
}

#[async_trait]
impl AsrProvider for VolcengineAsrAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        AsrModelCapabilities {
            languages: vec![Language::new("zh-CN"), Language::new("en")],
            streaming: true,
            batch: false,
            streaming_inputs: vec![AudioInputCapability {
                format: AudioFormat::Pcm,
                sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
                channels: ChannelSupport::Exact(vec![1, 2]),
                max_duration_ms: None,
                max_bytes: None,
            }],
            batch_inputs: vec![],
            batch_format_inference: false,
            audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
            interim_results: true,
            endpointing_modes: vec![],
            segment_flush: true,
            multi_segment_streaming: false,
            connection_reuse: ConnectionReuse::NotReusable,
            word_timestamps: true,
            speaker_diarization: false,
            confidence: false,
            code_switching: true,
            hot_words: true,
            context_prompt: true,
            provider_option_keys: vec![
                "enable_nonstream",
                "enable_itn",
                "enable_punc",
                "enable_ddc",
                "enable_speaker_info",
                "ssd_version",
                "show_utterances",
                "show_speech_rate",
                "show_volume",
                "enable_lid",
                "enable_emotion_detection",
                "enable_gender_detection",
                "result_type",
                "end_window_size",
                "vad_segment_duration",
                "force_to_speech_time",
                "enable_accelerate_text",
                "accelerate_score",
                "sensitive_words_filter",
                "output_zh_variant",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            max_duration_ms: None,
            default_flush_timeout_ms: Some(2000),
            source: CapabilitySource::Static,
            diagnostic_metadata: serde_json::Value::Null,
        }
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
        let model = format!("volcengine/{}", self.config.model);

        let connect_id = uuid::Uuid::new_v4().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();

        let mut ws_builder = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("X-Api-Key", &self.config.api_key);
        if let Some(ak) = &self.config.access_key {
            ws_builder = ws_builder.header("X-Api-Access-Key", ak);
        }
        let ws_request = ws_builder
            .header("X-Api-Resource-Id", &self.config.resource_id)
            .header("X-Api-Connect-Id", &connect_id)
            .header("X-Api-Request-Id", &request_id)
            .header("X-Api-Sequence", "-1")
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

        let client_payload = self.build_client_payload(&request);
        let full_request_frame = build_full_client_request(&client_payload)?;

        let (audio_tx, audio_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(64);

        let flush_timeout = request
            .options
            .flush_timeout
            .unwrap_or(Duration::from_secs(2));
        let final_result_scope = request.options.final_result_scope.clone();

        let adapter_trace_id = trace_id.clone();
        let adapter_model = model.clone();

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
                full_request_frame,
                audio_rx,
                event_tx,
                adapter_trace_id,
                adapter_model,
                flush_timeout,
                final_result_scope,
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
        .unwrap_or("openspeech.bytedance.com")
}

// ---------------------------------------------------------------------------
// Utterance deduplication
// ---------------------------------------------------------------------------

struct UtteranceDeduplicator {
    seen: HashSet<(i32, i32, String)>,
}

impl UtteranceDeduplicator {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    fn is_new(&mut self, u: &VolcengineUtterance) -> bool {
        let key = (u.start_time, u.end_time, u.text.clone());
        self.seen.insert(key)
    }
}

// ---------------------------------------------------------------------------
// Adapter task
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // justified: single-function duplex WebSocket adapter; splitting would scatter the session state machine across helpers
async fn adapter_task(
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    full_request_frame: Vec<u8>,
    mut audio_rx: mpsc::Receiver<AudioChunk>,
    event_tx: mpsc::Sender<AsrStreamEvent>,
    trace_id: String,
    model: String,
    flush_timeout: Duration,
    final_result_scope: FinalResultScope,
) {
    let (mut ws_write, mut ws_read) = ws_stream.split();

    if ws_write
        .send(tungstenite::Message::Binary(full_request_frame))
        .await
        .is_err()
    {
        let _ = event_tx
            .send(AsrStreamEvent::Error {
                trace_id: trace_id.clone(),
                error: AsrError::new(
                    AsrErrorCode::ProviderStreamError,
                    "failed to send full client request",
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

    let mut dedup = UtteranceDeduplicator::new();
    let mut segment_finalized = false;
    let mut flush_pending = false;
    let mut flush_started: Option<Instant> = None;
    let mut end_requested = false;
    let mut accumulated_text = String::new();
    let mut segment_idx = 0u32;
    let mut last_segment_text = String::new();
    let mut audio_duration_ms: u64 = 0;
    let mut segment_words: Vec<WordTimestamp> = Vec::new();

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
                        let is_last = matches!(chunk.boundary, AudioChunkBoundary::Flush | AudioChunkBoundary::End);
                        if is_last && !segment_finalized {
                            flush_pending = true;
                            flush_started = Some(Instant::now());
                        }
                        if matches!(chunk.boundary, AudioChunkBoundary::End) {
                            end_requested = true;
                        }
                        let frame = build_audio_frame(&chunk.data, is_last);
                        if ws_write.send(tungstenite::Message::Binary(frame)).await.is_err() {
                            let _ = event_tx.send(AsrStreamEvent::Error {
                                trace_id: trace_id.clone(),
                                error: AsrError::new(AsrErrorCode::ProviderStreamError, "WebSocket send failed"),
                                fatal: true,
                            }).await;
                            return;
                        }
                    }
                    None => {
                        if !segment_finalized && !flush_pending {
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
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        match parse_response(&data) {
                            Ok(VolcengineFrame::ServerResponse { payload, is_last, .. }) => {
                                if let Some(ref result) = payload.result {
                                    if let Some(duration) = payload.audio_info.as_ref().and_then(|a| a.duration) {
                                        audio_duration_ms = duration;
                                    }

                                    if let Some(ref utterances) = result.utterances {
                                        for u in utterances {
                                            if u.definite {
                                                if !dedup.is_new(u) {
                                                    continue;
                                                }
                                                if let Some(ws) = u.words.as_ref() {
                                                    for w in ws {
                                                        segment_words.push(WordTimestamp {
                                                            word: w.text.clone(),
                                                            start_ms: w.start_time as u64,
                                                            end_ms: w.end_time as u64,
                                                            confidence: None,
                                                        });
                                                    }
                                                }
                                                let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                                    trace_id: trace_id.clone(),
                                                    segment_id: Some(format!("seg-{segment_idx}")),
                                                    text: u.text.clone(),
                                                    stability: TranscriptStability::Committed,
                                                    update_kind: TranscriptUpdateKind::Snapshot,
                                                }).await;
                                                telemetry.on_transcript_update(None, false);
                                            } else {
                                                let _ = event_tx.send(AsrStreamEvent::TranscriptUpdate {
                                                    trace_id: trace_id.clone(),
                                                    segment_id: Some(format!("seg-{segment_idx}")),
                                                    text: u.text.clone(),
                                                    stability: TranscriptStability::Provisional,
                                                    update_kind: TranscriptUpdateKind::Snapshot,
                                                }).await;
                                                telemetry.on_transcript_update(None, false);
                                            }
                                        }
                                    }

                                    last_segment_text = result.text.clone();
                                }

                                if is_last && !segment_finalized {
                                    flush_pending = false;

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

                                    let final_words = std::mem::take(&mut segment_words);
                                    let _ = event_tx.send(AsrStreamEvent::AsrFinal {
                                        final_output: Box::new(AsrFinalOutput {
                                            trace_id: trace_id.clone(),
                                            segment_id: Some(format!("seg-{segment_idx}")),
                                            reason,
                                            result: TranscribeResult {
                                                text,
                                                language: Some(Language::new("zh-CN")),
                                                confidence: None,
                                                words: final_words,
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

                                    // Reinitialize for potential next segment
                                    telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                                    telemetry.on_started();

                                    if end_requested {
                                        let _ = ws_write.close().await;
                                        return;
                                    }

                                    segment_finalized = false;
                                    segment_idx += 1;
                                    segment_words.clear();
                                    dedup = UtteranceDeduplicator::new();
                                    last_segment_text.clear();
                                }
                            }
                            Ok(VolcengineFrame::ErrorResponse { code, message }) => {
                                let _ = event_tx.send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error: AsrError::new(
                                        AsrErrorCode::ProviderStreamError,
                                        format!("volcengine error {code}: {message}"),
                                    ),
                                    fatal: true,
                                }).await;
                                return;
                            }
                            Err(e) => {
                                let _ = event_tx.send(AsrStreamEvent::Error {
                                    trace_id: trace_id.clone(),
                                    error: e,
                                    fatal: true,
                                }).await;
                                return;
                            }
                        }
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => {
                        if !segment_finalized && flush_pending {
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
                if flush_pending && !segment_finalized {
                    telemetry.set_audio_duration_ms(audio_duration_ms);
                    emit_timeout_final(
                        &event_tx, &trace_id, segment_idx,
                        &last_segment_text, &mut accumulated_text,
                        &final_result_scope, audio_duration_ms, telemetry,
                    ).await;
                    flush_pending = false;
                    segment_words.clear();

                    if end_requested {
                        let _ = ws_write.close().await;
                        return;
                    }

                    segment_idx += 1;
                    segment_finalized = false;
                    dedup = UtteranceDeduplicator::new();
                    last_segment_text.clear();
                    telemetry = AsrTelemetryBuilder::new(trace_id.clone(), model.clone());
                    telemetry.on_started();
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
    telemetry: AsrTelemetryBuilder,
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
// Dedup tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utterance_dedup() {
        let mut dedup = UtteranceDeduplicator::new();
        let u = VolcengineUtterance {
            text: "hello".into(),
            definite: true,
            start_time: 0,
            end_time: 1000,
            words: None,
            additions: None,
        };
        assert!(dedup.is_new(&u));
        assert!(!dedup.is_new(&u));
    }

    #[test]
    fn utterance_dedup_different_text() {
        let mut dedup = UtteranceDeduplicator::new();
        let u1 = VolcengineUtterance {
            text: "hello".into(),
            definite: true,
            start_time: 0,
            end_time: 1000,
            words: None,
            additions: None,
        };
        let u2 = VolcengineUtterance {
            text: "world".into(),
            definite: true,
            start_time: 0,
            end_time: 1000,
            words: None,
            additions: None,
        };
        assert!(dedup.is_new(&u1));
        assert!(dedup.is_new(&u2));
    }

    #[test]
    fn host_extraction() {
        assert_eq!(
            extract_host("wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async"),
            "openspeech.bytedance.com"
        );
        assert_eq!(extract_host("ws://localhost:8080/test"), "localhost:8080");
    }

    #[test]
    fn build_client_payload_basic() {
        let config = VolcengineAsrConfig {
            model: "bigasr".into(),
            ws_url: "wss://test.com".into(),
            api_key: "key".into(),
            access_key: None,
            resource_id: "res".into(),
        };
        let adapter = VolcengineAsrAdapter::new(config);
        let request = StreamingTranscribeRequest {
            model: Some("volcengine/bigasr".into()),
            format: StreamingAudioFormat::Pcm16 {
                sample_rate_hz: 16000,
                channels: 1,
            },
            timeline: AudioTimelineMode::ContinuousRealtime,
            options: TranscribeOptions::default(),
            compatibility: CompatibilityPolicy::Coerce,
            provider_options: serde_json::Value::Null,
        };

        let payload = adapter.build_client_payload(&request);
        assert_eq!(payload["audio"]["rate"], 16000);
        assert_eq!(payload["audio"]["channel"], 1);
        assert_eq!(payload["request"]["model_name"], "bigmodel");
        assert_eq!(payload["request"]["show_utterances"], true);
        assert_eq!(payload["request"]["result_type"], "single");
    }

    #[test]
    fn build_client_payload_with_hot_words() {
        let config = VolcengineAsrConfig {
            model: "bigasr".into(),
            ws_url: "wss://test.com".into(),
            api_key: "key".into(),
            access_key: None,
            resource_id: "res".into(),
        };
        let adapter = VolcengineAsrAdapter::new(config);
        let options = TranscribeOptions {
            hot_words: vec!["热词1".into(), "热词2".into()],
            ..Default::default()
        };

        let request = StreamingTranscribeRequest {
            model: Some("volcengine/bigasr".into()),
            format: StreamingAudioFormat::Pcm16 {
                sample_rate_hz: 16000,
                channels: 1,
            },
            timeline: AudioTimelineMode::ContinuousRealtime,
            options,
            compatibility: CompatibilityPolicy::Coerce,
            provider_options: serde_json::Value::Null,
        };

        let payload = adapter.build_client_payload(&request);
        let context = payload["request"]["corpus"]["context"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(context).unwrap();
        assert_eq!(parsed["hotwords"][0]["word"], "热词1");
        assert_eq!(parsed["hotwords"][1]["word"], "热词2");
    }

    #[tokio::test]
    async fn volcengine_rejects_insecure_ws_url() {
        let adapter = VolcengineAsrAdapter::new(VolcengineAsrConfig {
            model: "bigasr".into(),
            ws_url: "ws://example.invalid/api/v3/sauc/bigmodel_async".into(),
            api_key: "key".into(),
            access_key: Some("access".into()),
            resource_id: "resource".into(),
        });
        let request = StreamingTranscribeRequest {
            model: Some("volcengine/bigasr".into()),
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

    #[test]
    fn build_client_payload_with_endpointing() {
        let config = VolcengineAsrConfig {
            model: "bigasr".into(),
            ws_url: "wss://test.com".into(),
            api_key: "key".into(),
            access_key: None,
            resource_id: "res".into(),
        };
        let adapter = VolcengineAsrAdapter::new(config);
        let options = TranscribeOptions {
            endpointing: Some(EndpointingOptions {
                mode: EndpointingMode::AcousticSilence,
                silence_timeout: Some(Duration::from_millis(500)),
            }),
            ..Default::default()
        };

        let request = StreamingTranscribeRequest {
            model: Some("volcengine/bigasr".into()),
            format: StreamingAudioFormat::Pcm16 {
                sample_rate_hz: 16000,
                channels: 1,
            },
            timeline: AudioTimelineMode::ContinuousRealtime,
            options,
            compatibility: CompatibilityPolicy::Coerce,
            provider_options: serde_json::Value::Null,
        };

        let payload = adapter.build_client_payload(&request);
        assert_eq!(payload["request"]["end_window_size"], 500);
    }
}
