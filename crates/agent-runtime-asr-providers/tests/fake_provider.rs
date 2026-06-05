use agent_runtime_asr_providers::error::AsrError;
use agent_runtime_asr_providers::streaming::{AsrEventStream, AsrStream};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct FakeAsrProvider {
    pub provider: String,
    pub model: String,
    pub caps: AsrModelCapabilities,
    pub languages: Vec<Language>,
}

impl FakeAsrProvider {
    pub fn volcengine() -> Arc<Self> {
        Arc::new(Self {
            provider: "volcengine".into(),
            model: "bigmodel_async".into(),
            caps: AsrModelCapabilities {
                model: "volcengine/bigmodel_async".into(),
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
            },
            languages: vec![Language::new("zh-CN"), Language::new("en")],
        })
    }

    pub fn aliyun() -> Arc<Self> {
        Arc::new(Self {
            provider: "aliyun".into(),
            model: "fun-asr-realtime".into(),
            caps: AsrModelCapabilities {
                model: "aliyun/fun-asr-realtime".into(),
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
            },
            languages: vec![
                Language::new("zh-CN"),
                Language::new("en"),
                Language::new("ja"),
            ],
        })
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

    async fn transcribe(&self, _request: TranscribeRequest) -> Result<TranscribeResult, AsrError> {
        Err(AsrError::unsupported_operation())
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        let (audio_tx, _audio_rx) = tokio::sync::mpsc::channel(64);
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(64);

        let _ = event_tx
            .send(AsrStreamEvent::Started {
                trace_id: "fake-trace".into(),
                model: self.caps.model.clone(),
            })
            .await;

        Ok(AsrStream::new(
            agent_runtime_asr_providers::streaming::AsrAudioSink::new(audio_tx),
            AsrEventStream::new(event_rx),
        ))
    }
}
