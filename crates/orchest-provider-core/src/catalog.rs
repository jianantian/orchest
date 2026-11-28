//! Multi-capability model catalog types (Batch 0).
//!
//! Pure data: static rows, filters, and projection onto
//! [`orchest_protocol::CapabilityDescriptor`]. Tables live in impl crates;
//! materialization / discovery live in the wall (`orchest-provider`).

use orchest_protocol::{
    AsrCapability, Capability, CapabilityDescriptor, CapabilityExt, CapabilitySource, CatalogEntry,
    ChatCapabilityExt, ErrorCode, Modality, ModelPricing, ProtocolError, VoiceSource,
};
use serde_json::Value;

const REQUIRED_CAPABILITIES: &str = "require_capabilities";

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

/// ASR capabilities and provider route recorded for a model.
#[derive(Debug, Clone)]
pub struct AsrCatalogExt {
    pub source: VoiceSource,
    pub protocol: &'static str,
    pub capabilities: &'static [AsrCapability],
}

impl AsrCatalogExt {
    pub const fn direct(protocol: &'static str, capabilities: &'static [AsrCapability]) -> Self {
        Self {
            source: VoiceSource::Direct,
            protocol,
            capabilities,
        }
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
    pub fn take_required_capabilities(
        &self,
        options: &mut Value,
    ) -> Result<Vec<String>, ProtocolError> {
        take_required_capabilities(options, |name| {
            self.capabilities
                .iter()
                .any(|capability| capability.name() == name)
        })
    }

    pub fn to_descriptor_ext(&self) -> CapabilityExt {
        let mut detail = serde_json::json!({
            "source": self.source,
            "protocol": self.protocol,
        });
        for capability in [
            AsrCapability::CommittedAppend,
            AsrCapability::ServerEndpointing,
            AsrCapability::ClientFinalize,
            AsrCapability::Words,
            AsrCapability::Speaker,
            AsrCapability::Confidence,
            AsrCapability::SpeechStarted,
            AsrCapability::Failover,
        ] {
            detail[capability.name()] = self.capabilities.contains(&capability).into();
        }
        CapabilityExt::Asr(detail)
    }
}

/// TTS provider route and optional audio alignment support.
#[derive(Debug, Clone)]
pub struct TtsCatalogExt {
    pub source: VoiceSource,
    pub protocol: &'static str,
    pub words: bool,
    pub duplex: bool,
    pub flush: bool,
    pub cancel: bool,
}

impl TtsCatalogExt {
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
    pub fn validate_options(&self, options: &mut Value) -> Result<(), ProtocolError> {
        take_required_capabilities(options, |name| name == "words" && self.words).map(|_| ())
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
    pub fn validate_duplex_options(&self, options: &mut Value) -> Result<(), ProtocolError> {
        if !options.is_null() && !options.is_object() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "TTS duplex options must be an object",
            ));
        }
        take_required_capabilities(options, |name| match name {
            "duplex" | "close" => self.duplex,
            "flush" => self.flush,
            "cancel" => self.cancel,
            "words" => self.words,
            _ => false,
        })
        .map(|_| ())
    }

    pub fn to_descriptor_ext(&self) -> CapabilityExt {
        CapabilityExt::Tts(serde_json::json!({
            "source": self.source,
            "protocol": self.protocol,
            "words": self.words,
            "flush": self.flush,
            "cancel": self.cancel,
            "close": self.duplex,
        }))
    }
}

/// Consume portable capability requirements before forwarding dialect options.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
fn take_required_capabilities(
    options: &mut Value,
    supports: impl Fn(&str) -> bool,
) -> Result<Vec<String>, ProtocolError> {
    let Some(requirements) = options
        .as_object_mut()
        .and_then(|options| options.remove(REQUIRED_CAPABILITIES))
    else {
        return Ok(Vec::new());
    };
    let requirements: Vec<String> = serde_json::from_value(requirements).map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidRequest,
            "options.require_capabilities must be an array of capability names",
        )
    })?;
    if let Some(missing) = requirements.iter().find(|name| !supports(name)) {
        return Err(ProtocolError::new(
            ErrorCode::UnsupportedOption,
            format!("voice model does not support capability {missing}"),
        ));
    }
    Ok(requirements)
}

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
    /// A public voice list rate, where applicable to this model and route.
    pub voice_pricing: Option<crate::voice::VoicePrice>,
    pub ext: CatalogExt,
}

impl ModelRecord {
    /// Description invariant: non-empty after trim.
    pub fn description_is_valid(&self) -> bool {
        !self.description.trim().is_empty()
    }

    /// Project this row onto the registry's queryable descriptor.
    pub fn to_descriptor(&self) -> CapabilityDescriptor {
        let desc = CapabilityDescriptor::new(self.provider, self.model, self.capability)
            .with_input_modalities(self.input_modalities.clone())
            .with_output_modalities(self.output_modalities.clone())
            .streaming(self.streaming)
            .tools(self.tools)
            .thinking(self.thinking)
            .duplex(self.duplex)
            .interruptible(self.interruptible)
            .default_for_provider(self.default_for_provider)
            .with_source(CapabilitySource::Static);

        let ext = match &self.ext {
            CatalogExt::Chat(c) => CapabilityExt::Chat(ChatCapabilityExt {
                max_output_tokens: c.max_output_tokens,
                context_window_size: Some(c.context_window),
                pricing: self.pricing.clone(),
                ..Default::default()
            }),
            CatalogExt::Asr(asr) => asr.to_descriptor_ext(),
            CatalogExt::Tts(tts) => tts.to_descriptor_ext(),
            _ => CapabilityExt::None,
        };
        desc.with_ext(ext)
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
    /// Require every listed ASR capability.
    pub asr_capabilities: Vec<AsrCapability>,
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
        if !self.asr_capabilities.is_empty()
            && !matches!(&r.ext, CatalogExt::Asr(asr)
                if self.asr_capabilities.iter().all(|capability| asr.capabilities.contains(capability)))
        {
            return false;
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

    #[test]
    fn portable_requirements_preserve_dialect_options() {
        let catalog = AsrCatalogExt::direct("test", &[AsrCapability::ClientFinalize]);
        let mut options = serde_json::json!({
            "require_capabilities": ["client_finalize"],
            "enable_endpoint_detection": false,
        });
        let requested = catalog.take_required_capabilities(&mut options).unwrap();
        assert_eq!(requested, ["client_finalize"]);
        assert_eq!(
            options,
            serde_json::json!({"enable_endpoint_detection": false})
        );
        let error = catalog
            .take_required_capabilities(&mut serde_json::json!({"require_capabilities": "speaker"}))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }

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
            voice_pricing: None,
            ext: CatalogExt::Asr(AsrCatalogExt::direct("test", &[])),
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
            voice_pricing: None,
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
