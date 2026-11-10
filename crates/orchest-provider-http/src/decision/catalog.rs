use std::sync::LazyLock;

use orchest_protocol::{Capability, Decision, Modality};
use orchest_provider_core::catalog::{CatalogExt, ModelRecord, ModelStatus};
use orchest_provider_core::registry::Entry;

use super::typesafe::{deployment, TypeSafeDecision};

static MODELS: LazyLock<Vec<ModelRecord>> = LazyLock::new(|| {
    [
        ("openrouter/~typesafe/jev-latest", "openrouter", "~typesafe/jev-latest", "Jev Latest", true,
            "Structured boolean, choice, and ordered score judgments via the OpenRouter Decisions alpha API."),
        ("openrouter/typesafe/jev-1.13", "openrouter", "typesafe/jev-1.13", "Jev 1.13", false,
            "Structured boolean, choice, and ordered score judgments via the OpenRouter Decisions alpha API."),
        ("vercel/typesafe-ai/jev", "vercel", "typesafe-ai/jev", "Jev", true,
            "Structured boolean, choice, and ordered score judgments via Vercel AI Gateway's TypeSafe-compatible Decisions API."),
    ].into_iter().map(|(id, provider, model, display_name, default_for_provider, description)| ModelRecord {
        id, provider, model, capability: Capability::Decision,
        display_name, description,
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
            // Invariant: every row of the static MODELS table names a provider
            // in `typesafe::deployment` (pinned by the decision catalog tests).
            let deployment =
                deployment(record.provider).expect("every decision row has a deployment");
            Entry::new(descriptor.clone(), move |config| {
                Ok(Box::new(TypeSafeDecision::new(
                    deployment,
                    config,
                    descriptor.clone(),
                )?) as Box<dyn Decision>)
            })
        })
        .collect()
}
