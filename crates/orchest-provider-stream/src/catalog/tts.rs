//! Static TTS models and the capabilities exposed by their adapters.

use std::sync::LazyLock;

use orchest_protocol::{Capability, Modality};
use orchest_provider_core::catalog::{CatalogExt, ModelRecord, ModelStatus};

pub static STREAM_TTS_MODELS: LazyLock<Vec<ModelRecord>> = LazyLock::new(|| {
    [
        (
            "volcengine",
            "tts",
            "volcengine/tts",
            "Volcengine TTS",
            "Volcengine OpenSpeech unidirectional WebSocket synthesis",
            crate::tts::volcengine::CATALOG_EXT,
        ),
        (
            "minimax",
            "speech-2.8-hd",
            "minimax/speech-2.8-hd",
            "MiniMax Speech 2.8 HD",
            "MiniMax WebSocket speech synthesis",
            crate::tts::minimax::CATALOG_EXT,
        ),
        (
            "aliyun",
            "cosyvoice-v2",
            "aliyun/cosyvoice-v2",
            "Aliyun CosyVoice v2",
            "DashScope CosyVoice WebSocket speech synthesis",
            crate::tts::aliyun::CATALOG_EXT,
        ),
    ]
    .into_iter()
    .map(
        |(provider, model, id, display_name, description, tts)| ModelRecord {
            id,
            provider,
            model,
            capability: Capability::Tts,
            display_name,
            description,
            input_modalities: vec![Modality::Text],
            output_modalities: vec![Modality::Audio],
            streaming: true,
            duplex: false,
            interruptible: false,
            tools: false,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: false,
            pricing: None,
            ext: CatalogExt::Tts(tts),
        },
    )
    .collect()
});

pub fn stream_tts_models() -> &'static [ModelRecord] {
    STREAM_TTS_MODELS.as_slice()
}
