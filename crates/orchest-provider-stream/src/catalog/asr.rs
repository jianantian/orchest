//! Streaming ASR catalog rows (Batch 1 / C1).
//!
//! Static `ModelRecord` table for WebSocket ASR dialects. Registry entries pin
//! `provider` + `model` from these rows; `from_provider_config` free functions
//! remain available for uncataloged passthrough.

use std::sync::LazyLock;

use orchest_protocol::{Capability, Modality};
use orchest_provider_core::catalog::{AsrCatalogExt, CatalogExt, ModelRecord, ModelStatus};

/// Stream-tier ASR models exposed for discovery and model-pinned registry entries.
pub static STREAM_ASR_MODELS: LazyLock<Vec<ModelRecord>> = LazyLock::new(|| {
    vec![
        // Source: docs/external/aliyun/asr-api-doc.md
        ModelRecord {
            id: "aliyun/fun-asr-realtime",
            provider: "aliyun",
            model: "fun-asr-realtime",
            capability: Capability::Asr,
            display_name: "Fun-ASR Realtime",
            description: "阿里云 DashScope 实时语音识别（Fun-ASR），WebSocket inference 双工流式",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        },
        // Source: docs/external/aliyun/asr-guideline.md
        ModelRecord {
            id: "aliyun/fun-asr-realtime-2026-02-28",
            provider: "aliyun",
            model: "fun-asr-realtime-2026-02-28",
            capability: Capability::Asr,
            display_name: "Fun-ASR Realtime 2026-02-28",
            description:
                "阿里云 DashScope Fun-ASR 实时语音识别快照版（2026-02-28），与 fun-asr-realtime 共用 inference WebSocket 方言",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: false,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        },
        // Source: docs/external/volceengine/asr.md
        ModelRecord {
            id: "volcengine/bigmodel",
            provider: "volcengine",
            model: "bigmodel",
            capability: Capability::Asr,
            display_name: "Volcengine Bigmodel ASR",
            description: "火山引擎 openspeech 大模型流式语音识别，WebSocket 双工二进制协议",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        },
        // Source: docs/archive/iteration/v0_9_6/asr-provider-guide.md
        ModelRecord {
            id: "deepgram/nova-3",
            provider: "deepgram",
            model: "nova-3",
            capability: Capability::Asr,
            display_name: "Deepgram Nova-3",
            description: "Deepgram Nova-3 实时语音识别，WebSocket v1/listen 双工流式",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        },
        // Source: docs/archive/iteration/v0_9_6/asr-provider-guide.md
        ModelRecord {
            id: "soniox/stt-rt-v5",
            provider: "soniox",
            model: "stt-rt-v5",
            capability: Capability::Asr,
            display_name: "Soniox STT RT v5",
            description: "Soniox 实时多语种/语码转换语音识别，WebSocket 双工流式",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        },
        // Source: docs/archive/iteration/v0_9_6/asr-provider-guide.md
        ModelRecord {
            id: "elevenlabs/scribe-v2-realtime",
            provider: "elevenlabs",
            model: "scribe-v2-realtime",
            capability: Capability::Asr,
            display_name: "ElevenLabs Scribe v2 Realtime",
            description: "ElevenLabs Scribe v2 实时语音识别，WebSocket 双工流式",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: true,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt {}),
        },
    ]
});

/// Borrow the stream ASR catalog rows.
pub fn stream_asr_models() -> &'static [ModelRecord] {
    STREAM_ASR_MODELS.as_slice()
}
