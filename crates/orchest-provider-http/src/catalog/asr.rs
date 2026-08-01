//! Batch/REST ASR catalog rows (Batch 1 / C1).
//!
//! Static `ModelRecord` table for HTTP one-shot ASR dialects. Registry entries
//! pin `provider` + `model` from these rows; `from_provider_config` free functions
//! remain available for uncataloged passthrough.

use std::sync::LazyLock;

use orchest_protocol::{Capability, Modality};
use orchest_provider_core::catalog::{AsrCatalogExt, CatalogExt, ModelRecord, ModelStatus};

/// HTTP-tier batch ASR models exposed for discovery and model-pinned registry entries.
pub static HTTP_ASR_MODELS: LazyLock<Vec<ModelRecord>> = LazyLock::new(|| {
    vec![
        // Source: docs/archive/iteration/v0_9_6/asr-provider-guide.md
        ModelRecord {
            id: "assemblyai/universal",
            provider: "assemblyai",
            model: "universal",
            capability: Capability::Asr,
            display_name: "AssemblyAI Universal",
            description: "AssemblyAI Universal 批量语音识别，REST submit+poll 一次性转写",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: false,
            duplex: false,
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
            id: "speechmatics/enhanced",
            provider: "speechmatics",
            model: "enhanced",
            capability: Capability::Asr,
            display_name: "Speechmatics Enhanced",
            description:
                "Speechmatics Enhanced 批量语音识别，REST multipart submit+poll 一次性转写",
            input_modalities: vec![Modality::Audio],
            output_modalities: vec![Modality::Text],
            streaming: false,
            duplex: false,
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

/// Borrow the HTTP ASR catalog rows.
pub fn http_asr_models() -> &'static [ModelRecord] {
    HTTP_ASR_MODELS.as_slice()
}
