use async_trait::async_trait;

use crate::error::AsrError;
use crate::streaming::AsrStream;
use crate::traits::AsrProvider;
use crate::types::{
    AsrModelCapabilities, AudioFormat, AudioInputCapability, AudioTimelineMode, CapabilitySource,
    ChannelSupport, ConnectionReuse, EndpointingMode, Language, SampleRateSupport,
    StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};

pub struct VolcengineAsrAdapter {
    model: String,
    api_key: String,
    api_url: Option<String>,
}

impl VolcengineAsrAdapter {
    pub fn new(model: String, api_key: String, api_url: Option<String>) -> Self {
        Self {
            model,
            api_key,
            api_url,
        }
    }
}

#[async_trait]
impl AsrProvider for VolcengineAsrAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> AsrModelCapabilities {
        AsrModelCapabilities {
            model: format!("volcengine/{}", self.model),
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
            provider_option_keys: vec![
                "resource_id".into(),
                "enable_itn".into(),
                "enable_punc".into(),
                "enable_speaker_info".into(),
                "show_utterances".into(),
                "end_window_size".into(),
            ],
            max_duration_ms: None,
            default_flush_timeout_ms: Some(5000),
            source: CapabilitySource::Static,
            diagnostic_metadata: serde_json::Value::Null,
        }
    }

    fn supported_languages(&self) -> &[Language] {
        // Static reference not possible with dynamic vec; capabilities() is the canonical source.
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
        todo!("volcengine streaming — implemented in issue 005")
    }
}
