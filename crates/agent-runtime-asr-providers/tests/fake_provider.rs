#![allow(dead_code, clippy::too_many_arguments)]

use agent_runtime_asr_providers::error::{AsrError, AsrErrorCode};
use agent_runtime_asr_providers::observability::AsrTelemetry;
use agent_runtime_asr_providers::streaming::{AsrAudioSink, AsrEventStream, AsrStream};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub enum FakeAdapterBehavior {
    Normal,
    FlushTimeout,
    ProviderEndpoint,
    ProviderEndpointThenCallerFlush,
    DuplicateCommitted,
    StreamAccumulate,
    LateProviderFinal,
}

#[derive(Clone)]
pub enum FakeTranscribeBehavior {
    Unsupported,
    Success,
    Never {
        entered: Arc<Mutex<Option<oneshot::Sender<()>>>>,
        cancelled: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    },
}

pub struct FakeAsrProvider {
    pub provider: String,
    pub model: String,
    pub caps: AsrModelCapabilities,
    pub languages: Vec<Language>,
    pub behavior: FakeAdapterBehavior,
    pub transcribe_behavior: FakeTranscribeBehavior,
    pub flush_timeout_override: Option<Duration>,
}

impl FakeAsrProvider {
    pub fn volcengine() -> Arc<Self> {
        Arc::new(Self {
            provider: "volcengine".into(),
            model: "bigasr".into(),
            caps: make_volcengine_caps(),
            languages: vec![Language::new("zh-CN"), Language::new("en")],
            behavior: FakeAdapterBehavior::Normal,
            transcribe_behavior: FakeTranscribeBehavior::Unsupported,
            flush_timeout_override: None,
        })
    }

    pub fn aliyun() -> Arc<Self> {
        Arc::new(Self {
            provider: "aliyun".into(),
            model: "fun-asr-realtime".into(),
            caps: make_aliyun_caps(),
            languages: vec![
                Language::new("zh-CN"),
                Language::new("en"),
                Language::new("ja"),
            ],
            behavior: FakeAdapterBehavior::Normal,
            transcribe_behavior: FakeTranscribeBehavior::Unsupported,
            flush_timeout_override: None,
        })
    }

    pub fn deepgram() -> Arc<Self> {
        Arc::new(Self {
            provider: "deepgram".into(),
            model: "nova-3".into(),
            caps: make_deepgram_caps(),
            languages: vec![Language::new("en"), Language::new("es")],
            behavior: FakeAdapterBehavior::Normal,
            transcribe_behavior: FakeTranscribeBehavior::Unsupported,
            flush_timeout_override: None,
        })
    }

    pub fn elevenlabs() -> Arc<Self> {
        Arc::new(Self {
            provider: "elevenlabs".into(),
            model: "scribe_v2_realtime".into(),
            caps: make_elevenlabs_caps(),
            languages: vec![Language::new("en"), Language::new("es")],
            behavior: FakeAdapterBehavior::Normal,
            transcribe_behavior: FakeTranscribeBehavior::Unsupported,
            flush_timeout_override: None,
        })
    }

    pub fn batch() -> Arc<Self> {
        Self::batch_with_format_inference(false)
    }

    pub fn batch_with_format_inference(batch_format_inference: bool) -> Arc<Self> {
        Arc::new(Self {
            provider: "fake".into(),
            model: "batch".into(),
            caps: make_batch_caps(batch_format_inference),
            languages: vec![Language::new("zh-CN"), Language::new("en")],
            behavior: FakeAdapterBehavior::Normal,
            transcribe_behavior: FakeTranscribeBehavior::Success,
            flush_timeout_override: None,
        })
    }

    pub fn batch_never_transcribes(
        entered: oneshot::Sender<()>,
        cancelled: oneshot::Sender<()>,
    ) -> Arc<Self> {
        Arc::new(Self {
            provider: "fake".into(),
            model: "batch".into(),
            caps: make_batch_caps(false),
            languages: vec![Language::new("zh-CN")],
            behavior: FakeAdapterBehavior::Normal,
            transcribe_behavior: FakeTranscribeBehavior::Never {
                entered: Arc::new(Mutex::new(Some(entered))),
                cancelled: Arc::new(Mutex::new(Some(cancelled))),
            },
            flush_timeout_override: None,
        })
    }

    pub fn with_behavior(behavior: FakeAdapterBehavior) -> Arc<Self> {
        Arc::new(Self {
            provider: "volcengine".into(),
            model: "bigasr".into(),
            caps: make_volcengine_caps(),
            languages: vec![Language::new("zh-CN")],
            behavior,
            transcribe_behavior: FakeTranscribeBehavior::Unsupported,
            flush_timeout_override: None,
        })
    }

    pub fn with_behavior_and_timeout(
        behavior: FakeAdapterBehavior,
        flush_timeout: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            provider: "volcengine".into(),
            model: "bigasr".into(),
            caps: make_volcengine_caps(),
            languages: vec![Language::new("zh-CN")],
            behavior,
            transcribe_behavior: FakeTranscribeBehavior::Unsupported,
            flush_timeout_override: Some(flush_timeout),
        })
    }
}

fn make_volcengine_caps() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![Language::new("zh-CN"), Language::new("en")],
        streaming: true,
        batch: false,
        streaming_inputs: vec![AudioInputCapability {
            format: AudioFormat::Pcm,
            sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
            channels: ChannelSupport::Exact(vec![1]),
            max_duration_ms: None,
            max_bytes: None,
        }],
        batch_inputs: vec![],
        batch_format_inference: false,
        audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
        interim_results: true,
        endpointing_modes: vec![EndpointingMode::NaturalSegmenting],
        segment_flush: true,
        multi_segment_streaming: true,
        connection_reuse: ConnectionReuse::NotReusable,
        word_timestamps: false,
        speaker_diarization: false,
        confidence: false,
        code_switching: false,
        hot_words: true,
        context_prompt: true,
        provider_option_keys: vec!["resource_id".into(), "enable_itn".into()],
        max_duration_ms: None,
        default_flush_timeout_ms: Some(5000),
        source: CapabilitySource::Static,
        diagnostic_metadata: Value::Null,
    }
}

fn make_aliyun_caps() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![
            Language::new("zh-CN"),
            Language::new("en"),
            Language::new("ja"),
        ],
        streaming: true,
        batch: false,
        streaming_inputs: vec![AudioInputCapability {
            format: AudioFormat::Pcm,
            sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
            channels: ChannelSupport::Exact(vec![1]),
            max_duration_ms: None,
            max_bytes: None,
        }],
        batch_inputs: vec![],
        batch_format_inference: false,
        audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
        interim_results: true,
        endpointing_modes: vec![EndpointingMode::AcousticSilence],
        segment_flush: true,
        multi_segment_streaming: false,
        connection_reuse: ConnectionReuse::ReusableAfterProviderTaskFinished,
        word_timestamps: false,
        speaker_diarization: false,
        confidence: false,
        code_switching: false,
        hot_words: true,
        context_prompt: false,
        provider_option_keys: vec!["max_sentence_silence".into()],
        max_duration_ms: None,
        default_flush_timeout_ms: Some(5000),
        source: CapabilitySource::Static,
        diagnostic_metadata: Value::Null,
    }
}

fn make_deepgram_caps() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![Language::new("en"), Language::new("es")],
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
        provider_option_keys: vec!["smart_format".into()],
        max_duration_ms: None,
        default_flush_timeout_ms: Some(3000),
        source: CapabilitySource::Static,
        diagnostic_metadata: serde_json::json!({"provider": "deepgram"}),
    }
}

fn make_elevenlabs_caps() -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![
            Language::new("en"),
            Language::new("es"),
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
            "keyterms".into(),
            "include_language_detection".into(),
            "commit_strategy".into(),
            "vad_silence_threshold_secs".into(),
        ],
        max_duration_ms: None,
        default_flush_timeout_ms: Some(3000),
        source: CapabilitySource::Static,
        diagnostic_metadata: serde_json::json!({"provider": "elevenlabs"}),
    }
}

fn make_batch_caps(batch_format_inference: bool) -> AsrModelCapabilities {
    AsrModelCapabilities {
        languages: vec![Language::new("zh-CN"), Language::new("en")],
        streaming: false,
        batch: true,
        streaming_inputs: vec![],
        batch_inputs: vec![
            AudioInputCapability {
                format: AudioFormat::Pcm,
                sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
                channels: ChannelSupport::Exact(vec![1]),
                max_duration_ms: None,
                max_bytes: Some(1024),
            },
            AudioInputCapability {
                format: AudioFormat::Mp3,
                sample_rates_hz: SampleRateSupport::Any,
                channels: ChannelSupport::Any,
                max_duration_ms: None,
                max_bytes: Some(1024 * 1024),
            },
            AudioInputCapability {
                format: AudioFormat::Wav,
                sample_rates_hz: SampleRateSupport::Any,
                channels: ChannelSupport::Any,
                max_duration_ms: None,
                max_bytes: Some(1024 * 1024),
            },
        ],
        batch_format_inference,
        audio_timeline_modes: vec![],
        interim_results: false,
        endpointing_modes: vec![],
        segment_flush: false,
        multi_segment_streaming: false,
        connection_reuse: ConnectionReuse::NotReusable,
        word_timestamps: true,
        speaker_diarization: false,
        confidence: true,
        code_switching: false,
        hot_words: true,
        context_prompt: true,
        provider_option_keys: vec!["batch_hint".into()],
        max_duration_ms: Some(60_000),
        default_flush_timeout_ms: None,
        source: CapabilitySource::Static,
        diagnostic_metadata: Value::Null,
    }
}

fn make_telemetry(trace_id: &str, model: &str) -> AsrTelemetry {
    AsrTelemetry {
        trace_id: trace_id.into(),
        model: model.into(),
        language: Some("zh-CN".into()),
        audio_duration_ms: 0,
        latency_first_update_ms: None,
        latency_final_ms: 0,
        update_rollback_count: 0,
        confidence_avg: None,
        cost_estimate_micros: None,
        network_region: None,
        upstream_status: None,
        option_adjustment_count: 0,
    }
}

fn make_final_output(
    trace_id: &str,
    model: &str,
    text: &str,
    reason: AsrFinalReason,
    segment_id: Option<String>,
) -> AsrFinalOutput {
    AsrFinalOutput {
        trace_id: trace_id.into(),
        segment_id,
        reason,
        result: TranscribeResult {
            text: text.into(),
            language: Some(Language::new("zh-CN")),
            confidence: None,
            words: vec![],
            speakers: vec![],
            audio_duration_ms: 0,
            processing_latency_ms: 0,
            usage: AsrUsage::default(),
            option_adjustments: vec![],
            telemetry: make_telemetry(trace_id, model),
        },
    }
}

#[async_trait]
impl AsrProvider for FakeAsrProvider {
    fn provider_name(&self) -> &str {
        &self.provider
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        self.caps.clone()
    }

    fn supported_languages(&self) -> &[Language] {
        &self.languages
    }

    async fn transcribe(&self, request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        match &self.transcribe_behavior {
            FakeTranscribeBehavior::Unsupported => Err(AsrError::unsupported_operation()),
            FakeTranscribeBehavior::Success => Ok(make_transcribe_result(
                request.options.trace_id.as_deref().unwrap_or("fake-trace"),
                &format!("{}/{}", self.provider, self.model),
                "one-shot transcript",
            )),
            FakeTranscribeBehavior::Never { entered, cancelled } => {
                send_once(entered);
                let _guard = CancelGuard {
                    cancelled: Arc::clone(cancelled),
                };
                std::future::pending::<()>().await;
                unreachable!("pending future never resolves")
            }
        }
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        let (audio_tx, audio_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(64);

        let trace_id = request
            .options
            .trace_id
            .clone()
            .unwrap_or_else(|| "fake-trace".into());
        let model = format!("{}/{}", self.provider, self.model);
        let behavior = self.behavior.clone();
        let final_result_scope = request.options.final_result_scope.clone();
        let flush_timeout = self
            .flush_timeout_override
            .or(request.options.flush_timeout)
            .unwrap_or(Duration::from_secs(5));

        tokio::spawn(adapter_task(
            audio_rx,
            event_tx,
            trace_id,
            model,
            behavior,
            final_result_scope,
            flush_timeout,
        ));

        Ok(AsrStream::new(
            AsrAudioSink::new(audio_tx),
            AsrEventStream::new(event_rx),
        ))
    }
}

struct CancelGuard {
    cancelled: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        send_once(&self.cancelled);
    }
}

fn send_once(slot: &Arc<Mutex<Option<oneshot::Sender<()>>>>) {
    if let Ok(mut guard) = slot.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
}

fn make_transcribe_result(trace_id: &str, model: &str, text: &str) -> TranscribeResult {
    TranscribeResult {
        text: text.into(),
        language: Some(Language::new("zh-CN")),
        confidence: Some(0.98),
        words: vec![],
        speakers: vec![],
        audio_duration_ms: 1000,
        processing_latency_ms: 25,
        usage: AsrUsage {
            audio_duration_ms: 1000,
            billable_duration_ms: Some(1000),
            input_bytes: None,
            transcript_chars: Some(text.len() as u64),
            cost_estimate_micros: None,
        },
        option_adjustments: vec![],
        telemetry: make_telemetry(trace_id, model),
    }
}

async fn adapter_task(
    mut audio_rx: mpsc::Receiver<AudioChunk>,
    event_tx: mpsc::Sender<AsrStreamEvent>,
    trace_id: String,
    model: String,
    behavior: FakeAdapterBehavior,
    final_result_scope: FinalResultScope,
    flush_timeout: Duration,
) {
    let _ = event_tx
        .send(AsrStreamEvent::RouteSelected {
            trace_id: trace_id.clone(),
            model: model.clone(),
        })
        .await;
    let _ = event_tx
        .send(AsrStreamEvent::Started {
            trace_id: trace_id.clone(),
            model: model.clone(),
        })
        .await;

    let mut segment_text = String::new();
    let mut accumulated_text = String::new();
    let mut segment_finalized = false;
    let mut segment_counter = 0u32;
    let mut flush_pending = false;
    let mut committed_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    match behavior {
        FakeAdapterBehavior::ProviderEndpoint => {
            handle_provider_endpoint_mode(
                &mut audio_rx,
                &event_tx,
                &trace_id,
                &model,
                &final_result_scope,
                &mut accumulated_text,
            )
            .await;
            return;
        }
        FakeAdapterBehavior::ProviderEndpointThenCallerFlush => {
            handle_provider_endpoint_then_flush(
                &mut audio_rx,
                &event_tx,
                &trace_id,
                &model,
                &final_result_scope,
                &mut accumulated_text,
            )
            .await;
            return;
        }
        FakeAdapterBehavior::DuplicateCommitted => {
            handle_duplicate_committed(&mut audio_rx, &event_tx, &trace_id, &model).await;
            return;
        }
        FakeAdapterBehavior::LateProviderFinal => {
            handle_late_provider_final(&mut audio_rx, &event_tx, &trace_id, &model, flush_timeout)
                .await;
            return;
        }
        _ => {}
    }

    loop {
        let chunk = if flush_pending {
            let timeout_fut = tokio::time::sleep(flush_timeout);
            tokio::select! {
                chunk = audio_rx.recv() => chunk,
                () = timeout_fut => {
                    if !segment_finalized {
                        let text = result_text(&final_result_scope, &segment_text, &accumulated_text);
                        let output = make_final_output(
                            &trace_id, &model, &text,
                            AsrFinalReason::Timeout,
                            Some(format!("seg-{}", segment_counter)),
                        );
                        let _ = event_tx.send(AsrStreamEvent::AsrFinal {
                            final_output: Box::new(output),
                        }).await;
                        segment_finalized = true;
                        accumulated_text.push_str(&segment_text);
                        segment_text.clear();
                        segment_counter += 1;
                        flush_pending = false;
                    }
                    continue;
                }
            }
        } else {
            audio_rx.recv().await
        };

        let Some(chunk) = chunk else {
            if !segment_finalized && flush_pending {
                let text = result_text(&final_result_scope, &segment_text, &accumulated_text);
                let output = make_final_output(
                    &trace_id,
                    &model,
                    &text,
                    AsrFinalReason::Timeout,
                    Some(format!("seg-{}", segment_counter)),
                );
                let _ = event_tx
                    .send(AsrStreamEvent::AsrFinal {
                        final_output: Box::new(output),
                    })
                    .await;
            } else if !segment_finalized {
                let _ = event_tx
                    .send(AsrStreamEvent::Error {
                        trace_id: trace_id.clone(),
                        error: AsrError::new(AsrErrorCode::Cancelled, "audio sink dropped"),
                        fatal: true,
                    })
                    .await;
            }
            return;
        };

        match chunk.boundary {
            AudioChunkBoundary::None => {
                if !chunk.data.is_empty() {
                    segment_text.push_str("transcribed ");
                    let update_text = format!("{}transcribed ", segment_text);
                    let dedup_key = format!("committed-{}", update_text.len());

                    let _ = event_tx
                        .send(AsrStreamEvent::TranscriptUpdate {
                            trace_id: trace_id.clone(),
                            segment_id: Some(format!("seg-{}", segment_counter)),
                            text: update_text.clone(),
                            stability: TranscriptStability::Provisional,
                            update_kind: TranscriptUpdateKind::Snapshot,
                        })
                        .await;

                    if !committed_keys.contains(&dedup_key) {
                        committed_keys.insert(dedup_key);
                    }
                }
            }
            AudioChunkBoundary::Flush => {
                if matches!(behavior, FakeAdapterBehavior::FlushTimeout) {
                    flush_pending = true;
                    continue;
                }

                if !segment_finalized {
                    let text = result_text(&final_result_scope, &segment_text, &accumulated_text);
                    let output = make_final_output(
                        &trace_id,
                        &model,
                        &text,
                        AsrFinalReason::CallerFlush,
                        Some(format!("seg-{}", segment_counter)),
                    );
                    let _ = event_tx
                        .send(AsrStreamEvent::AsrFinal {
                            final_output: Box::new(output),
                        })
                        .await;
                    accumulated_text.push_str(&segment_text);
                    segment_text.clear();
                    segment_counter += 1;
                    segment_finalized = false;
                    committed_keys.clear();
                }
            }
            AudioChunkBoundary::End => {
                if !segment_finalized {
                    let text = result_text(&final_result_scope, &segment_text, &accumulated_text);
                    let output = make_final_output(
                        &trace_id,
                        &model,
                        &text,
                        AsrFinalReason::CallerEnd,
                        Some(format!("seg-{}", segment_counter)),
                    );
                    let _ = event_tx
                        .send(AsrStreamEvent::AsrFinal {
                            final_output: Box::new(output),
                        })
                        .await;
                }
                return;
            }
        }
    }
}

async fn handle_provider_endpoint_mode(
    audio_rx: &mut mpsc::Receiver<AudioChunk>,
    event_tx: &mpsc::Sender<AsrStreamEvent>,
    trace_id: &str,
    model: &str,
    final_result_scope: &FinalResultScope,
    accumulated_text: &mut String,
) {
    let mut segment_text = String::new();
    while let Some(chunk) = audio_rx.recv().await {
        if !chunk.data.is_empty() {
            segment_text.push_str("speech ");

            let _ = event_tx
                .send(AsrStreamEvent::TranscriptUpdate {
                    trace_id: trace_id.into(),
                    segment_id: Some("seg-0".into()),
                    text: segment_text.clone(),
                    stability: TranscriptStability::Provisional,
                    update_kind: TranscriptUpdateKind::Snapshot,
                })
                .await;
        }

        if chunk.data.len() >= 200 || matches!(chunk.boundary, AudioChunkBoundary::End) {
            let _ = event_tx
                .send(AsrStreamEvent::EndOfSpeech {
                    trace_id: trace_id.into(),
                    segment_id: Some("seg-0".into()),
                })
                .await;

            let text = result_text(final_result_scope, &segment_text, accumulated_text);
            accumulated_text.push_str(&segment_text);
            let output = make_final_output(
                trace_id,
                model,
                &text,
                AsrFinalReason::ProviderEndpoint,
                Some("seg-0".into()),
            );
            let _ = event_tx
                .send(AsrStreamEvent::AsrFinal {
                    final_output: Box::new(output),
                })
                .await;
            return;
        }
    }
}

async fn handle_provider_endpoint_then_flush(
    audio_rx: &mut mpsc::Receiver<AudioChunk>,
    event_tx: &mpsc::Sender<AsrStreamEvent>,
    trace_id: &str,
    model: &str,
    final_result_scope: &FinalResultScope,
    accumulated_text: &mut String,
) {
    let mut segment_text = String::new();
    let mut finalized = false;
    while let Some(chunk) = audio_rx.recv().await {
        if !chunk.data.is_empty() && !finalized {
            segment_text.push_str("speech ");
        }

        if !finalized && chunk.data.len() >= 200 {
            let _ = event_tx
                .send(AsrStreamEvent::EndOfSpeech {
                    trace_id: trace_id.into(),
                    segment_id: Some("seg-0".into()),
                })
                .await;

            let text = result_text(final_result_scope, &segment_text, accumulated_text);
            accumulated_text.push_str(&segment_text);
            let output = make_final_output(
                trace_id,
                model,
                &text,
                AsrFinalReason::ProviderEndpoint,
                Some("seg-0".into()),
            );
            let _ = event_tx
                .send(AsrStreamEvent::AsrFinal {
                    final_output: Box::new(output),
                })
                .await;
            finalized = true;
        }

        if matches!(chunk.boundary, AudioChunkBoundary::Flush) && finalized {
            // Already finalized — no duplicate AsrFinal
            continue;
        }

        if matches!(chunk.boundary, AudioChunkBoundary::End) {
            return;
        }
    }
}

async fn handle_duplicate_committed(
    audio_rx: &mut mpsc::Receiver<AudioChunk>,
    event_tx: &mpsc::Sender<AsrStreamEvent>,
    trace_id: &str,
    model: &str,
) {
    let mut committed_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut count = 0u32;

    while let Some(chunk) = audio_rx.recv().await {
        if !chunk.data.is_empty() {
            let text = format!("committed-segment-{}", count);
            let dedup_key = text.clone();

            // Emit same committed update twice (simulating provider replay)
            for _ in 0..2 {
                if !committed_keys.contains(&dedup_key) {
                    committed_keys.insert(dedup_key.clone());
                    let _ = event_tx
                        .send(AsrStreamEvent::TranscriptUpdate {
                            trace_id: trace_id.into(),
                            segment_id: Some("seg-0".into()),
                            text: text.clone(),
                            stability: TranscriptStability::Committed,
                            update_kind: TranscriptUpdateKind::Snapshot,
                        })
                        .await;
                }
                // Second one is silently deduplicated
            }
            count += 1;
        }

        if matches!(chunk.boundary, AudioChunkBoundary::End) {
            let output = make_final_output(
                trace_id,
                model,
                "committed-segment-0",
                AsrFinalReason::CallerEnd,
                Some("seg-0".into()),
            );
            let _ = event_tx
                .send(AsrStreamEvent::AsrFinal {
                    final_output: Box::new(output),
                })
                .await;
            return;
        }
    }
}

async fn handle_late_provider_final(
    audio_rx: &mut mpsc::Receiver<AudioChunk>,
    event_tx: &mpsc::Sender<AsrStreamEvent>,
    trace_id: &str,
    model: &str,
    flush_timeout: Duration,
) {
    let mut flush_received = false;

    while let Some(chunk) = audio_rx.recv().await {
        if matches!(chunk.boundary, AudioChunkBoundary::Flush) {
            flush_received = true;
            break;
        }
    }

    if flush_received {
        // Wait for timeout, emit Timeout final
        tokio::time::sleep(flush_timeout).await;
        let output = make_final_output(
            trace_id,
            model,
            "timeout-text",
            AsrFinalReason::Timeout,
            Some("seg-0".into()),
        );
        let _ = event_tx
            .send(AsrStreamEvent::AsrFinal {
                final_output: Box::new(output),
            })
            .await;

        // Simulate late provider final — should NOT be emitted as a second AsrFinal
        // (adapter suppressed it; we just don't send it)

        // Drain remaining
        while let Some(chunk) = audio_rx.recv().await {
            if matches!(chunk.boundary, AudioChunkBoundary::End) {
                return;
            }
        }
    }
}

fn result_text(scope: &FinalResultScope, segment: &str, accumulated: &str) -> String {
    match scope {
        FinalResultScope::Segment => segment.to_string(),
        FinalResultScope::Stream => format!("{}{}", accumulated, segment),
    }
}
