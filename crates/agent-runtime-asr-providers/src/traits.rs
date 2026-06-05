use async_trait::async_trait;

use crate::error::AsrError;
use crate::streaming::AsrStream;
use crate::types::{
    AsrModelCapabilities, Language, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};

#[async_trait]
pub trait AsrProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> AsrModelCapabilities;
    fn supported_languages(&self) -> &[Language];

    async fn transcribe(&self, request: TranscribeRequest) -> Result<TranscribeResult, AsrError>;

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError>;
}
