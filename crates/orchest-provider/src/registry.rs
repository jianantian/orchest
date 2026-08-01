//! The multi-capability, descriptor-queryable registry.
//!
//! Reconciles the two legacy shapes into one mechanism (PRD §From Here To There):
//! - LLM `ProviderRegistry` (factory-by-provider-name) → folded into identity
//!   pick (`.provider(..)` / `.id("provider/model")`).
//! - ASR `AsrRouter.select_for_*` (filter candidates, then pick) → generalized
//!   into the capability query (`.accepts(..).thinking()…` then select/list).
//!
//! One [`Query`] mechanism serves capability-query **and** identity-pick and lets
//! them mix (e.g. "bidirectional ASR from Volcengine"); it supports both
//! pick-one ([`Query::select`]) and list-then-choose ([`Query::list`]).

use orchest_protocol::{
    Asr, Capability, ChatModel, ErrorCode, GenTask, Modality, ProtocolError, RealtimeSession, Tts,
};
use orchest_provider_core::registry::{Entry, ProviderConfig};

/// The registry: one typed bucket of [`Entry`]s per capability. Entries are
/// **static descriptors + factories** — descriptors are queried before any
/// provider is instantiated.
#[derive(Default)]
pub struct Registry {
    chat: Vec<Entry<Box<dyn ChatModel>>>,
    asr: Vec<Entry<Box<dyn Asr>>>,
    tts: Vec<Entry<Box<dyn Tts>>>,
    realtime: Vec<Entry<Box<dyn RealtimeSession>>>,
    gen: Vec<Entry<Box<dyn GenTask>>>,
}

impl Registry {
    /// An empty registry (no impls). The mechanism-only constructor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the registry from whatever impl crates are enabled by features.
    /// With the default feature set (none) this is empty — the "mechanism-only
    /// build". Issues 005/006/007 fill the impl crates' entry functions.
    pub fn with_builtin() -> Self {
        #[allow(unused_mut)]
        let mut reg = Self::new();
        #[cfg(feature = "http")]
        {
            reg.chat.extend(orchest_provider_http::chat_entries());
            reg.asr.extend(orchest_provider_http::asr_entries());
            reg.tts.extend(orchest_provider_http::tts_entries());
            reg.gen.extend(orchest_provider_http::gen_entries());
        }
        #[cfg(feature = "stream")]
        {
            reg.asr.extend(orchest_provider_stream::asr_entries());
            reg.tts.extend(orchest_provider_stream::tts_entries());
            reg.realtime
                .extend(orchest_provider_stream::realtime_entries());
        }
        #[cfg(feature = "visual")]
        {
            reg.gen.extend(orchest_provider_visual::gen_entries());
        }
        reg
    }

    // --- registration (used by tests/fixtures and, later, the impl crates) ---

    pub fn register_chat(&mut self, entry: Entry<Box<dyn ChatModel>>) {
        self.chat.push(entry);
    }
    pub fn register_asr(&mut self, entry: Entry<Box<dyn Asr>>) {
        self.asr.push(entry);
    }
    pub fn register_tts(&mut self, entry: Entry<Box<dyn Tts>>) {
        self.tts.push(entry);
    }
    pub fn register_realtime(&mut self, entry: Entry<Box<dyn RealtimeSession>>) {
        self.realtime.push(entry);
    }
    pub fn register_gen(&mut self, entry: Entry<Box<dyn GenTask>>) {
        self.gen.push(entry);
    }

    // --- capability query entry points (the fluent selection surface) ---

    pub fn chat(&self) -> Query<'_, Box<dyn ChatModel>> {
        Query::new(&self.chat, Capability::Chat)
    }
    pub fn asr(&self) -> Query<'_, Box<dyn Asr>> {
        Query::new(&self.asr, Capability::Asr)
    }
    pub fn tts(&self) -> Query<'_, Box<dyn Tts>> {
        Query::new(&self.tts, Capability::Tts)
    }
    pub fn realtime(&self) -> Query<'_, Box<dyn RealtimeSession>> {
        Query::new(&self.realtime, Capability::Realtime)
    }
    pub fn gen(&self) -> Query<'_, Box<dyn GenTask>> {
        Query::new(&self.gen, Capability::GenTask)
    }
}

/// Selection filters accumulated by the fluent builder.
#[derive(Default, Clone)]
struct Filters {
    provider: Option<String>,
    model: Option<String>,
    accepts: Vec<Modality>,
    emits: Vec<Modality>,
    streaming: Option<bool>,
    tools: Option<bool>,
    thinking: Option<bool>,
    duplex: Option<bool>,
    interruptible: Option<bool>,
}

/// A capability-scoped selection builder. Filters narrow the candidate set;
/// [`Query::select`] picks one (deterministically), [`Query::list`] returns all.
pub struct Query<'r, H> {
    entries: &'r [Entry<H>],
    capability: Capability,
    f: Filters,
}

impl<'r, H> Query<'r, H> {
    fn new(entries: &'r [Entry<H>], capability: Capability) -> Self {
        Self {
            entries,
            capability,
            f: Filters::default(),
        }
    }

    /// Restrict to a vendor (identity dimension). Mixes with capability filters.
    #[must_use]
    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.f.provider = Some(provider.into());
        self
    }

    /// Identity pick by `"provider/model"` (or bare `"model"`). Folds the LLM
    /// factory-by-name lookup in.
    #[must_use]
    pub fn id(mut self, id: &str) -> Self {
        match id.split_once('/') {
            Some((provider, model)) => {
                self.f.provider = Some(provider.to_string());
                self.f.model = Some(model.to_string());
            }
            None => self.f.model = Some(id.to_string()),
        }
        self
    }

    /// Require the model to accept all of these input modalities.
    #[must_use]
    pub fn accepts(mut self, modalities: impl IntoIterator<Item = Modality>) -> Self {
        self.f.accepts.extend(modalities);
        self
    }

    /// Require the model to emit all of these output modalities.
    #[must_use]
    pub fn emits(mut self, modalities: impl IntoIterator<Item = Modality>) -> Self {
        self.f.emits.extend(modalities);
        self
    }

    #[must_use]
    pub fn streaming(mut self) -> Self {
        self.f.streaming = Some(true);
        self
    }

    #[must_use]
    pub fn tools(mut self) -> Self {
        self.f.tools = Some(true);
        self
    }

    #[must_use]
    pub fn thinking(mut self) -> Self {
        self.f.thinking = Some(true);
        self
    }

    /// Bidirectional/duplex session (realtime duplex, tts-duplex, asr-streaming).
    #[must_use]
    pub fn bidirectional(mut self) -> Self {
        self.f.duplex = Some(true);
        self
    }

    #[must_use]
    pub fn interruptible(mut self) -> Self {
        self.f.interruptible = Some(true);
        self
    }

    fn matches(&self, entry: &Entry<H>) -> bool {
        let d = &entry.descriptor;
        if let Some(p) = &self.f.provider {
            if d.provider.as_ref() != p.as_str() {
                return false;
            }
        }
        if let Some(m) = &self.f.model {
            if d.model.as_ref() != m.as_str() {
                return false;
            }
        }
        if !d.accepts(&self.f.accepts) {
            return false;
        }
        if !d.emits(&self.f.emits) {
            return false;
        }
        if matches!(self.f.streaming, Some(true)) && !d.streaming {
            return false;
        }
        if matches!(self.f.tools, Some(true)) && !d.tools {
            return false;
        }
        if matches!(self.f.thinking, Some(true)) && !d.thinking {
            return false;
        }
        if matches!(self.f.duplex, Some(true)) && !d.duplex {
            return false;
        }
        if matches!(self.f.interruptible, Some(true)) && !d.interruptible {
            return false;
        }
        true
    }

    /// All matching entries, sorted deterministically by `(provider, model)`
    /// (list-then-choose). Mirrors `AsrRouter`'s candidate collection.
    pub fn list(&self) -> Vec<&'r Entry<H>> {
        let mut out: Vec<&Entry<H>> = self.entries.iter().filter(|e| self.matches(e)).collect();
        out.sort_by(|a, b| {
            (a.descriptor.provider.as_ref(), a.descriptor.model.as_ref())
                .cmp(&(b.descriptor.provider.as_ref(), b.descriptor.model.as_ref()))
        });
        out
    }

    /// Pick one matching entry per C2: unique match, or unique
    /// `default_for_provider` among multi-matches. Multi-match with zero or
    /// multiple defaults is `NoMatchingProvider` (message notes ambiguity).
    /// `list()` sort is unchanged and never a silent multi-match winner.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn select(&self) -> Result<&'r Entry<H>, ProtocolError> {
        let matches = self.list();
        match matches.as_slice() {
            [] => Err(ProtocolError::new(
                ErrorCode::NoMatchingProvider,
                format!(
                    "no registered {:?} provider matches the selection",
                    self.capability
                ),
            )),
            [one] => Ok(*one),
            many => {
                let defaults: Vec<_> = many
                    .iter()
                    .copied()
                    .filter(|e| e.descriptor.default_for_provider)
                    .collect();
                match defaults.as_slice() {
                    [one] => Ok(*one),
                    [] | [_, _, ..] => Err(ProtocolError::new(
                        ErrorCode::NoMatchingProvider,
                        format!(
                            "ambiguous {:?} selection: {} matches without a unique default_for_provider; narrow with .id(\"provider/model\") or .provider(..)",
                            self.capability,
                            many.len()
                        ),
                    )),
                }
            }
        }
    }

    /// Convenience: select one entry and instantiate it with `config`.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn build(&self, config: &ProviderConfig) -> Result<H, ProtocolError> {
        self.select()?.instantiate(config)
    }
}
