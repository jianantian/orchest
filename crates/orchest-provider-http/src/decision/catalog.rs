use std::sync::LazyLock;

use orchest_protocol::{Capability, Decision, Modality};
use orchest_provider_core::catalog::{CatalogExt, ModelRecord, ModelStatus};
use orchest_provider_core::registry::Entry;

use super::openrouter::OpenRouterDecision;

static MODELS: LazyLock<Vec<ModelRecord>> = LazyLock::new(|| {
    [
        ("openrouter/~typesafe/jev-latest", "~typesafe/jev-latest", "Jev Latest", true),
        ("openrouter/typesafe/jev-1.13", "typesafe/jev-1.13", "Jev 1.13", false),
    ].into_iter().map(|(id, model, display_name, default_for_provider)| ModelRecord {
        id, provider: "openrouter", model, capability: Capability::Decision,
        display_name, description: "Structured boolean, choice, and ordered score judgments via the OpenRouter Decisions alpha API.",
        input_modalities: vec![Modality::Text], output_modalities: vec![Modality::Text],
        streaming: false, duplex: false, interruptible: false, tools: false, thinking: false,
        status: ModelStatus::Preview, default_for_provider, pricing: None, ext: CatalogExt::None,
    }).collect()
});

/// Credential-free Decision catalog; the latest alias may change upstream.
pub fn decision_models() -> &'static [ModelRecord] {
    &MODELS
}

/// Model-pinned factories: selection determines identity, config supplies credentials.
#[allow(clippy::result_large_err)] // justified: registry factories use the shared structured ProtocolError
pub fn decision_entries() -> Vec<Entry<Box<dyn Decision>>> {
    decision_models()
        .iter()
        .map(|record| {
            let descriptor = record.to_descriptor();
            Entry::new(descriptor.clone(), move |config| {
                Ok(
                    Box::new(OpenRouterDecision::new(config, descriptor.clone())?)
                        as Box<dyn Decision>,
                )
            })
        })
        .collect()
}
