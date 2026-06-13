use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::mpsc;

use agent_runtime_tts_providers::{
    validate_direct_model_selector, AudioData, AudioFormat, DuplexSynthesizeRequest,
    ListVoicesRequest, SynthesizeRequest, SynthesizeResult, TtsDuplexStream, TtsError,
    TtsErrorCode, TtsModelCapabilities, TtsOperation, TtsOutputStream, TtsProvider, TtsStreamEvent,
    TtsStreamSummary, TtsTelemetry, TtsUsage, VoiceCatalogSource, VoiceGender, VoiceInfo,
    VoiceKind,
};

pub struct FakeTtsProvider {
    provider: String,
    model: String,
    capabilities: TtsModelCapabilities,
    reject_semantic_controls: bool,
}

impl FakeTtsProvider {
    pub fn new(provider: &str, model: &str) -> Self {
        Self {
            provider: provider.to_owned(),
            model: model.to_owned(),
            capabilities: TtsModelCapabilities::default(),
            reject_semantic_controls: false,
        }
    }

    pub fn with_batch_only(mut self) -> Self {
        self.capabilities.batch_synthesis = true;
        self.capabilities.batch_output_formats = vec![AudioFormat::Mp3];
        self.capabilities.languages = vec![agent_runtime_tts_providers::Language::new("en-US")];
        self
    }

    pub fn with_streams(mut self) -> Self {
        self = self.with_batch_only();
        self.capabilities.single_streaming = true;
        self.capabilities.stream_output_formats = vec![AudioFormat::Mp3];
        self
    }

    pub fn with_duplex(mut self) -> Self {
        self = self.with_streams();
        self.capabilities.duplex_streaming = true;
        self
    }

    pub fn reject_semantic_controls(mut self) -> Self {
        self.reject_semantic_controls = true;
        self
    }

    fn telemetry(&self, trace_id: Option<String>, operation: TtsOperation) -> TtsTelemetry {
        TtsTelemetry {
            trace_id: trace_id.unwrap_or_else(|| "trace-direct".to_owned()),
            provider: self.provider.clone(),
            model: self.model.clone(),
            operation,
            voice_id: Some("voice-a".to_owned()),
            input_chars: 5,
            output_bytes: Some(3),
            first_audio_latency_ms: Some(1),
            final_latency_ms: Some(2),
            option_adjustment_count: 0,
            status: None,
            upstream_code: None,
        }
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        vec![
            VoiceInfo {
                provider: self.provider.clone(),
                model: self.model.clone(),
                id: "voice-a".to_owned(),
                display_name: "Voice A".to_owned(),
                kind: VoiceKind::System,
                gender: Some(VoiceGender::Neutral),
                languages: vec![agent_runtime_tts_providers::Language::new("en-US")],
                is_custom: false,
                supports_instruction: false,
                supports_emotion: false,
                supports_style: false,
                supports_cloning: false,
                supports_design: false,
                source: VoiceCatalogSource::StaticCatalog,
                provider_metadata: serde_json::Value::Null,
            },
            VoiceInfo {
                provider: self.provider.clone(),
                model: self.model.clone(),
                id: "custom-a".to_owned(),
                display_name: "Custom A".to_owned(),
                kind: VoiceKind::Custom,
                gender: None,
                languages: vec![agent_runtime_tts_providers::Language::new("en-US")],
                is_custom: true,
                supports_instruction: false,
                supports_emotion: false,
                supports_style: false,
                supports_cloning: true,
                supports_design: false,
                source: VoiceCatalogSource::CallerConfig,
                provider_metadata: serde_json::Value::Null,
            },
        ]
    }
}

#[async_trait]
impl TtsProvider for FakeTtsProvider {
    fn provider_name(&self) -> &str {
        &self.provider
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> TtsModelCapabilities {
        self.capabilities.clone()
    }

    async fn synthesize(&self, request: SynthesizeRequest) -> Result<SynthesizeResult, TtsError> {
        validate_direct_model_selector(self.provider_name(), self.model_name(), &request.model)?;
        if self.reject_semantic_controls
            && (request.controls.instruction.is_some()
                || request.controls.emotion.is_some()
                || request.controls.style.is_some())
        {
            return Err(TtsError::new(
                TtsErrorCode::InvalidRequest,
                "semantic controls reached fake provider",
            ));
        }
        Ok(SynthesizeResult {
            audio: AudioData::Bytes(Bytes::from_static(b"abc")),
            format: AudioFormat::Mp3,
            duration_ms: Some(100),
            usage: TtsUsage::for_text("hello", Some(3)),
            option_adjustments: Vec::new(),
            provider_metadata: serde_json::Value::Null,
            telemetry: self.telemetry(request.trace_id, TtsOperation::Batch),
        })
    }

    async fn stream_synthesize(
        &self,
        request: agent_runtime_tts_providers::StreamSynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError> {
        validate_direct_model_selector(self.provider_name(), self.model_name(), &request.model)?;
        let (tx, rx) = mpsc::channel(8);
        let trace_id = request
            .trace_id
            .unwrap_or_else(|| "trace-direct-stream".to_owned());
        let voice = self.voices().remove(0);
        let summary = TtsStreamSummary {
            format: AudioFormat::Mp3,
            duration_ms: Some(100),
            usage: TtsUsage::for_text("hello", Some(3)),
            option_adjustments: Vec::new(),
            provider_metadata: serde_json::Value::Null,
            telemetry: self.telemetry(Some(trace_id.clone()), TtsOperation::SingleStream),
        };
        let provider = self.provider.clone();
        let model = self.model.clone();
        tokio::spawn(async move {
            let _ = tx
                .send(TtsStreamEvent::Started {
                    trace_id: trace_id.clone(),
                    provider,
                    model,
                    voice,
                })
                .await;
            let _ = tx
                .send(TtsStreamEvent::TextDelta {
                    trace_id: trace_id.clone(),
                    text: "hello".to_owned(),
                    sequence: 0,
                    is_final: true,
                })
                .await;
            let _ = tx
                .send(TtsStreamEvent::AudioChunk {
                    trace_id: trace_id.clone(),
                    data: Bytes::from_static(b"abc"),
                    format: AudioFormat::Mp3,
                    sequence: 0,
                })
                .await;
            let _ = tx
                .send(TtsStreamEvent::Completed { trace_id, summary })
                .await;
        });
        Ok(TtsOutputStream::new(rx))
    }

    async fn start_duplex_stream(
        &self,
        request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError> {
        validate_direct_model_selector(self.provider_name(), self.model_name(), &request.model)?;
        if !self.capabilities.duplex_streaming {
            return Err(TtsError::unsupported_operation());
        }

        let (input_tx, mut input_rx) = mpsc::channel::<agent_runtime_tts_providers::TextChunk>(8);
        let (event_tx, event_rx) = mpsc::channel(8);
        let trace_id = request
            .trace_id
            .unwrap_or_else(|| "trace-duplex".to_owned());
        let provider = self.provider.clone();
        let model = self.model.clone();
        let voice = self.voices().remove(0);
        tokio::spawn(async move {
            if event_tx
                .send(TtsStreamEvent::Started {
                    trace_id: trace_id.clone(),
                    provider,
                    model: model.clone(),
                    voice,
                })
                .await
                .is_err()
            {
                return;
            }

            let mut text_sequence = 0;
            let mut audio_sequence = 0;
            let mut input_chars = 0;
            while let Some(chunk) = input_rx.recv().await {
                if chunk.is_final {
                    let summary = TtsStreamSummary {
                        format: AudioFormat::Mp3,
                        duration_ms: Some(100),
                        usage: TtsUsage {
                            input_chars,
                            billable_chars: Some(input_chars),
                            audio_duration_ms: Some(100),
                            output_bytes: Some(audio_sequence * 3),
                            cost_estimate_micros: None,
                        },
                        option_adjustments: Vec::new(),
                        provider_metadata: serde_json::Value::Null,
                        telemetry: TtsTelemetry {
                            trace_id: trace_id.clone(),
                            provider: "fake".to_owned(),
                            model,
                            operation: TtsOperation::DuplexStream,
                            voice_id: Some("voice-a".to_owned()),
                            input_chars,
                            output_bytes: Some(audio_sequence * 3),
                            first_audio_latency_ms: Some(1),
                            final_latency_ms: Some(2),
                            option_adjustment_count: 0,
                            status: None,
                            upstream_code: None,
                        },
                    };
                    let _ = event_tx
                        .send(TtsStreamEvent::Completed { trace_id, summary })
                        .await;
                    return;
                }

                let chars = chunk.text.chars().count() as u64;
                input_chars += chars;
                if event_tx
                    .send(TtsStreamEvent::TextAccepted {
                        trace_id: trace_id.clone(),
                        chars,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
                if event_tx
                    .send(TtsStreamEvent::TextDelta {
                        trace_id: trace_id.clone(),
                        text: chunk.text,
                        sequence: text_sequence,
                        is_final: false,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
                text_sequence += 1;
                if event_tx
                    .send(TtsStreamEvent::AudioChunk {
                        trace_id: trace_id.clone(),
                        data: Bytes::from_static(b"abc"),
                        format: AudioFormat::Mp3,
                        sequence: audio_sequence,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
                audio_sequence += 1;
            }

            let _ = event_tx
                .send(TtsStreamEvent::Error {
                    trace_id,
                    error: TtsError::new(
                        TtsErrorCode::Cancelled,
                        "duplex input dropped before final chunk",
                    ),
                    fatal: true,
                })
                .await;
        });
        Ok(TtsDuplexStream::new(input_tx, event_rx))
    }

    async fn list_voices(&self, _request: ListVoicesRequest) -> Result<Vec<VoiceInfo>, TtsError> {
        Ok(self.voices())
    }
}
