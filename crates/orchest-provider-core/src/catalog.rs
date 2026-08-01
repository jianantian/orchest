//! Multi-capability model catalog types (Batch 0).
//!
//! Pure data: static rows, filters, and projection onto
//! [`orchest_protocol::CapabilityDescriptor`]. Tables live in impl crates;
//! materialization / discovery live in the wall (`orchest-provider`).

use orchest_protocol::{
    Capability, CapabilityDescriptor, CapabilityExt, CapabilitySource, CatalogEntry,
    ChatCapabilityExt, Modality, ModelPricing,
};

/// Lifecycle / stability of a cataloged model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelStatus {
    Stable,
    Snapshot,
    Preview,
    Deprecated,
}

/// Capability-specific catalog detail. Closed enum; payload types live here.
#[derive(Debug, Clone)]
pub enum CatalogExt {
    None,
    Chat(ChatCatalogExt),
    Asr(AsrCatalogExt),
    Tts(TtsCatalogExt),
    Realtime(RealtimeCatalogExt),
    GenTask(GenCatalogExt),
}

/// Chat-specific catalog fields used by Batch 0 projection from `LlmModelEntry`.
#[derive(Debug, Clone)]
pub struct ChatCatalogExt {
    pub context_window: u64,
    pub max_output_tokens: Option<u32>,
    pub max_input_tokens: Option<u64>,
    pub thinking_max_tokens: Option<u32>,
}

/// ASR-specific catalog detail (stub for Batch 0).
#[derive(Debug, Clone, Default)]
pub struct AsrCatalogExt {}

/// TTS-specific catalog detail (stub for Batch 0).
#[derive(Debug, Clone, Default)]
pub struct TtsCatalogExt {}

/// Realtime-specific catalog detail (stub for Batch 0).
#[derive(Debug, Clone, Default)]
pub struct RealtimeCatalogExt {}

/// GenTask-specific catalog detail (stub for Batch 0).
#[derive(Debug, Clone, Default)]
pub struct GenCatalogExt {}

/// Canonical static catalog row for any capability.
#[derive(Debug, Clone)]
pub struct ModelRecord {
    /// Full catalog id: `"provider/model"`.
    pub id: &'static str,
    pub provider: &'static str,
    pub model: &'static str,
    pub capability: Capability,
    pub display_name: &'static str,
    /// REQUIRED free-text description (non-empty after trim).
    pub description: &'static str,
    pub input_modalities: Vec<Modality>,
    pub output_modalities: Vec<Modality>,
    pub streaming: bool,
    pub duplex: bool,
    pub interruptible: bool,
    pub tools: bool,
    pub thinking: bool,
    pub status: ModelStatus,
    /// At most one default per (capability, provider) across enabled features.
    pub default_for_provider: bool,
    pub pricing: Option<ModelPricing>,
    pub ext: CatalogExt,
}

impl ModelRecord {
    /// Description invariant: non-empty after trim.
    pub fn description_is_valid(&self) -> bool {
        !self.description.trim().is_empty()
    }

    /// Project this row onto the registry's queryable descriptor.
    pub fn to_descriptor(&self) -> CapabilityDescriptor {
        let mut desc = CapabilityDescriptor::new(self.provider, self.model, self.capability)
            .with_input_modalities(self.input_modalities.clone())
            .with_output_modalities(self.output_modalities.clone())
            .streaming(self.streaming)
            .tools(self.tools)
            .thinking(self.thinking)
            .duplex(self.duplex)
            .interruptible(self.interruptible)
            .default_for_provider(self.default_for_provider)
            .with_source(CapabilitySource::Static);

        if let CatalogExt::Chat(c) = &self.ext {
            desc = desc.with_ext(CapabilityExt::Chat(ChatCapabilityExt {
                max_output_tokens: c.max_output_tokens,
                context_window_size: Some(c.context_window),
                pricing: self.pricing.clone(),
                ..Default::default()
            }));
        }

        desc
    }
}

impl CatalogEntry for ModelRecord {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.to_descriptor()
    }
}

/// Filters for credential-free catalog discovery.
#[derive(Debug, Clone, Default)]
pub struct ModelFilter {
    pub capability: Option<Capability>,
    pub provider: Option<&'static str>,
    pub streaming: Option<bool>,
    pub duplex: Option<bool>,
    pub interruptible: Option<bool>,
    pub tools: Option<bool>,
    pub thinking: Option<bool>,
    pub status: Option<ModelStatus>,
    pub accepts: Option<Vec<Modality>>,
    pub emits: Option<Vec<Modality>>,
    /// When false (default), omit Deprecated rows from human discovery helpers.
    pub include_deprecated: bool,
}

impl ModelFilter {
    /// Whether `r` satisfies this filter.
    pub fn matches(&self, r: &ModelRecord) -> bool {
        if let Some(capability) = self.capability {
            if r.capability != capability {
                return false;
            }
        }
        if let Some(provider) = self.provider {
            if r.provider != provider {
                return false;
            }
        }
        if let Some(streaming) = self.streaming {
            if r.streaming != streaming {
                return false;
            }
        }
        if let Some(duplex) = self.duplex {
            if r.duplex != duplex {
                return false;
            }
        }
        if let Some(interruptible) = self.interruptible {
            if r.interruptible != interruptible {
                return false;
            }
        }
        if let Some(tools) = self.tools {
            if r.tools != tools {
                return false;
            }
        }
        if let Some(thinking) = self.thinking {
            if r.thinking != thinking {
                return false;
            }
        }
        if let Some(status) = self.status {
            if r.status != status {
                return false;
            }
        }
        if !self.include_deprecated && r.status == ModelStatus::Deprecated {
            return false;
        }
        if let Some(accepts) = &self.accepts {
            for modality in accepts {
                if !r.input_modalities.contains(modality) {
                    return false;
                }
            }
        }
        if let Some(emits) = &self.emits {
            for modality in emits {
                if !r.output_modalities.contains(modality) {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::{Capability, Modality};

    fn sample(description: &'static str) -> ModelRecord {
        ModelRecord {
            id: "aliyun/fun-asr-realtime",
            provider: "aliyun",
            model: "fun-asr-realtime",
            capability: Capability::Asr,
            display_name: "Fun-ASR Realtime",
            description,
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
        }
    }

    #[test]
    fn description_must_be_non_empty() {
        assert!(sample("streaming ASR").description_is_valid());
        assert!(!sample("").description_is_valid());
        assert!(!sample("   ").description_is_valid());
    }

    #[test]
    fn to_descriptor_copies_default_flag_and_source_static() {
        let d = sample("ok").to_descriptor();
        assert!(d.default_for_provider);
        assert_eq!(d.source, orchest_protocol::CapabilitySource::Static);
        assert_eq!(d.model.as_ref(), "fun-asr-realtime");
        assert!(d.streaming && d.duplex);
    }

    #[test]
    fn to_descriptor_projects_chat_ext_and_pricing() {
        let pricing = ModelPricing::flat_text("USD", 1.0, 2.0);
        let record = ModelRecord {
            id: "openai/gpt-4o",
            provider: "openai",
            model: "gpt-4o",
            capability: Capability::Chat,
            display_name: "GPT-4o",
            description: "chat model",
            input_modalities: vec![Modality::Text],
            output_modalities: vec![Modality::Text],
            streaming: true,
            duplex: false,
            interruptible: false,
            tools: true,
            thinking: false,
            status: ModelStatus::Stable,
            default_for_provider: true,
            pricing: Some(pricing.clone()),
            ext: CatalogExt::Chat(ChatCatalogExt {
                context_window: 128_000,
                max_output_tokens: Some(16_384),
                max_input_tokens: None,
                thinking_max_tokens: None,
            }),
        };

        let d = record.to_descriptor();
        assert_eq!(d.source, CapabilitySource::Static);
        match &d.ext {
            CapabilityExt::Chat(chat) => {
                assert_eq!(chat.context_window_size, Some(128_000));
                assert_eq!(chat.max_output_tokens, Some(16_384));
                let projected = chat.pricing.as_ref().expect("pricing projected");
                assert_eq!(projected.currency, pricing.currency);
                assert_eq!(projected.tiers.len(), 1);
                assert_eq!(
                    projected.tiers[0].rates.text_input_per_million,
                    pricing.tiers[0].rates.text_input_per_million
                );
                assert_eq!(
                    projected.tiers[0].rates.text_output_per_million,
                    pricing.tiers[0].rates.text_output_per_million
                );
            }
            other => panic!("expected CapabilityExt::Chat, got {other:?}"),
        }
    }
}
