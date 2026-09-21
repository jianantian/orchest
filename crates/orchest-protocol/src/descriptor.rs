//! Unified capability descriptor — the queryable core the registry filters on,
//! plus a typed per-capability extension slot that preserves detail.
//!
//! Designed in `docs/archive/iteration/v0_9_12/issues/001-protocol-design/design.md` §2.
//! The core holds **only** what the registry queries; rich per-capability detail
//! (e.g. `AsrModelCapabilities`' endpointing/diarization) rides in [`CapabilityExt`]
//! and is folded in by the modality-migration issues (006/007), not flattened.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::options::{CacheCapability, ModelPricing, ReasoningCapability};

/// Where a capability fact came from. **Single, canonical definition** — replaces
/// the verbatim duplicate that used to live in `agent-runtime-asr-providers`
/// (`types.rs:416`). Serialized as-is (PascalCase) to match the spine's original
/// `agent-runtime-model` form, which `core/node/py` already consume.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CapabilitySource {
    #[default]
    Static,
    ProviderMetadata,
    Assumed,
}

/// A content modality a model can accept or produce. Lifted from the LLM catalog
/// (`agent-runtime-providers::catalog::Modality`) into the spine so the modality
/// bit lives in the same struct as `thinking`/`tools` (PRD §Starting Point).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
}

/// The capability primitive a descriptor describes. One descriptor = one
/// `(capability, provider, model)` row in the registry (Issue 004).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    Chat,
    /// Atomic structured judgments, independent of chat generation.
    Decision,
    Asr,
    Tts,
    Realtime,
    GenTask,
    VoiceManagement,
}

/// The queryable core. Available **statically** (no credentials, no network) so
/// the registry filters before instantiating a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub provider: Cow<'static, str>,
    pub model: Cow<'static, str>,
    pub capability: Capability,
    /// Content types the model accepts (folds catalog `input_modalities`).
    pub input_modalities: Vec<Modality>,
    /// Content types the model produces (folds catalog `output_modalities`).
    pub output_modalities: Vec<Modality>,
    pub streaming: bool,
    /// Tool/function calling (was `ModelCapabilities.tool_use`).
    pub tools: bool,
    /// Reasoning/thinking mode (catalog `thinking.is_some()` ∪
    /// `ModelCapabilities.reasoning.supported`).
    pub thinking: bool,
    /// Bidirectional session: realtime duplex / tts-duplex / asr-streaming.
    pub duplex: bool,
    /// Mid-stream barge-in (realtime).
    pub interruptible: bool,
    /// Whether this model is the default selection for its (capability, provider).
    #[serde(default)]
    pub default_for_provider: bool,
    pub source: CapabilitySource,
    /// Typed per-capability detail. Not flattened into the core.
    #[serde(default)]
    pub ext: CapabilityExt,
}

impl CapabilityDescriptor {
    /// Construct a minimal descriptor; refine with the builder methods.
    pub fn new(
        provider: impl Into<Cow<'static, str>>,
        model: impl Into<Cow<'static, str>>,
        capability: Capability,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            capability,
            input_modalities: vec![Modality::Text],
            output_modalities: vec![Modality::Text],
            streaming: false,
            tools: false,
            thinking: false,
            duplex: false,
            interruptible: false,
            default_for_provider: false,
            source: CapabilitySource::Static,
            ext: CapabilityExt::None,
        }
    }

    /// Does this model accept **all** of the requested input modalities? The
    /// primitive the registry's `.accepts([..])` filter is built on (Issue 004).
    pub fn accepts(&self, modalities: &[Modality]) -> bool {
        modalities.iter().all(|m| self.input_modalities.contains(m))
    }

    /// Does this model produce **all** of the requested output modalities?
    pub fn emits(&self, modalities: &[Modality]) -> bool {
        modalities
            .iter()
            .all(|m| self.output_modalities.contains(m))
    }

    #[must_use]
    pub fn with_input_modalities(mut self, modalities: impl Into<Vec<Modality>>) -> Self {
        self.input_modalities = modalities.into();
        self
    }

    #[must_use]
    pub fn with_output_modalities(mut self, modalities: impl Into<Vec<Modality>>) -> Self {
        self.output_modalities = modalities.into();
        self
    }

    #[must_use]
    pub fn streaming(mut self, yes: bool) -> Self {
        self.streaming = yes;
        self
    }

    #[must_use]
    pub fn tools(mut self, yes: bool) -> Self {
        self.tools = yes;
        self
    }

    #[must_use]
    pub fn thinking(mut self, yes: bool) -> Self {
        self.thinking = yes;
        self
    }

    #[must_use]
    pub fn duplex(mut self, yes: bool) -> Self {
        self.duplex = yes;
        self
    }

    #[must_use]
    pub fn interruptible(mut self, yes: bool) -> Self {
        self.interruptible = yes;
        self
    }

    #[must_use]
    pub fn default_for_provider(mut self, v: bool) -> Self {
        self.default_for_provider = v;
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: CapabilitySource) -> Self {
        self.source = source;
        self
    }

    #[must_use]
    pub fn with_ext(mut self, ext: CapabilityExt) -> Self {
        self.ext = ext;
        self
    }
}

/// Typed per-capability extension. The chat detail lives in the spine today; the
/// modality-specific structs (`AsrModelCapabilities`, `TtsModelCapabilities`, …)
/// fold into the `Asr`/`Tts`/… variants when their crates migrate onto the spine
/// (Issues 006/007). Until then those variants carry the provider's raw detail
/// as `Value` so no information is lost.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum CapabilityExt {
    #[default]
    None,
    Chat(ChatCapabilityExt),
    /// Folds `AsrModelCapabilities` in Issue 006.
    Asr(Value),
    /// Folds `TtsModelCapabilities` in Issue 006.
    Tts(Value),
    Realtime(Value),
    GenTask(Value),
}

/// Chat-capability detail the spine owns today: reasoning efforts, prompt cache,
/// parallel tool use, pricing, context window. (The queryable booleans are in the
/// core; this is the preserved detail behind them.)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCapabilityExt {
    pub parallel_tool_use: bool,
    pub reasoning: ReasoningCapability,
    pub prompt_cache: CacheCapability,
    pub max_output_tokens: Option<u32>,
    pub context_window_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricing>,
}

/// Static catalog producer — implemented by per-impl-crate catalog rows so the
/// registry (Issue 004) can enumerate descriptors **before** instantiating any
/// provider. `LlmModelEntry` (`agent-runtime-providers::catalog`) becomes a
/// `CatalogEntry` in Issue 005.
pub trait CatalogEntry {
    fn descriptor(&self) -> CapabilityDescriptor;
}
