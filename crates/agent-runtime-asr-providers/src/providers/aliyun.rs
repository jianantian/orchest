use async_trait::async_trait;

use crate::error::AsrError;
use crate::streaming::AsrStream;
use crate::traits::AsrProvider;
use crate::types::{
    AsrModelCapabilities, AudioFormat, AudioInputCapability, AudioTimelineMode, CapabilitySource,
    ChannelSupport, ConnectionReuse, EndpointingMode, Language, SampleRateSupport,
    StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};

pub struct AliyunAsrAdapter {
    model: String,
    api_key: String,
    api_url: Option<String>,
    region: Option<String>,
}

impl AliyunAsrAdapter {
    pub fn new(
        model: String,
        api_key: String,
        api_url: Option<String>,
        region: Option<String>,
    ) -> Self {
        Self {
            model,
            api_key,
            api_url,
            region,
        }
    }
}

#[async_trait]
impl AsrProvider for AliyunAsrAdapter {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        AsrModelCapabilities {
            model: format!("aliyun/{}", self.model),
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
        _request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        let _ = &self.api_key;
        let _ = &self.api_url;
        let _ = &self.region;
        todo!("aliyun streaming — implemented in issue 006")
    }
}
