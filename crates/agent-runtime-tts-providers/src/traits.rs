use async_trait::async_trait;

use crate::error::TtsError;
use crate::streaming::{TtsDuplexStream, TtsOutputStream};
use crate::types::{
    CloneVoiceRequest, CloneVoiceResponse, DesignVoiceRequest, DesignVoiceResponse,
    DuplexSynthesizeRequest, ListVoicesRequest, SynthesizeRequest, SynthesizeResult,
    TtsModelCapabilities, VoiceInfo, VoiceKind,
};

#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> TtsModelCapabilities;

    async fn synthesize(&self, request: SynthesizeRequest) -> Result<SynthesizeResult, TtsError>;

    async fn stream_synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError>;

    async fn start_duplex_stream(
        &self,
        request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError>;

    async fn list_voices(&self, request: ListVoicesRequest) -> Result<Vec<VoiceInfo>, TtsError>;
}

/// 音色管理接口 \u2014\u2014 与 `TtsProvider` 平行,只对 Minimax(及未来同形 provider)
/// 适用。设计来源:迭代 v0.9.10 issue 005 / 设计文档 §3.5、§七 Q2-A。
///
/// `clone_voice` 与 `design_voice` 需要事先通过 `files::upload_file` 拿到
/// audio file_id;`delete_voice` 仅支持 `Cloned` / `Designed` 两种音色,
/// `System` / `Custom` 返回 `UnsupportedOperation`。
#[async_trait]
pub trait VoiceManager: Send + Sync {
    async fn clone_voice(&self, req: CloneVoiceRequest) -> Result<CloneVoiceResponse, TtsError>;
    async fn design_voice(&self, req: DesignVoiceRequest) -> Result<DesignVoiceResponse, TtsError>;
    async fn delete_voice(&self, voice_id: &str, kind: VoiceKind) -> Result<(), TtsError>;
}
