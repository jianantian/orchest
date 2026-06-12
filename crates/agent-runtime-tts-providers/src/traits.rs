use async_trait::async_trait;

use crate::error::TtsError;
use crate::streaming::{TtsDuplexStream, TtsOutputStream};
use crate::types::{
    DuplexSynthesizeRequest, ListVoicesRequest, StreamSynthesizeRequest, SynthesizeRequest,
    SynthesizeResult, TtsModelCapabilities, VoiceInfo,
};

#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> TtsModelCapabilities;

    async fn synthesize(&self, request: SynthesizeRequest) -> Result<SynthesizeResult, TtsError>;

    async fn stream_synthesize(
        &self,
        request: StreamSynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError>;

    async fn start_duplex_stream(
        &self,
        request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError>;

    async fn list_voices(&self, request: ListVoicesRequest) -> Result<Vec<VoiceInfo>, TtsError>;
}
