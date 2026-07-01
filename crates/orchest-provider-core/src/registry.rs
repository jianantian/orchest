//! Registry entry + factory types (shared between the wall and the impl crates).
//!
//! These live in core — not in `orchest-provider` (the wall) — so the impl
//! crates can produce entries without depending on the wall (which would be a
//! cycle: the wall depends on the impl crates behind features). The wall
//! (`orchest-provider`, Issue 004) collects `Entry<H>`s and layers the
//! descriptor-queryable selection builder + vendor facade on top.

use std::sync::Arc;

use orchest_protocol::{CapabilityDescriptor, ProtocolError};

/// Runtime configuration handed to a factory to instantiate a provider.
/// Generalizes the LLM `ProviderFactory::create_adapter(model, max_tokens,
/// api_key, api_url)` signature (which did not generalize across capabilities)
/// into one config carrying dialect-specific knobs in `options`.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub options: serde_json::Value,
}

impl ProviderConfig {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn with_api_url(mut self, url: impl Into<String>) -> Self {
        self.api_url = Some(url.into());
        self
    }

    #[must_use]
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }
}

/// A factory that instantiates a capability handle `H` (e.g. `Box<dyn ChatModel>`)
/// from a [`ProviderConfig`]. The unified replacement for `ProviderFactory`.
pub type Factory<H> = Arc<dyn Fn(&ProviderConfig) -> Result<H, ProtocolError> + Send + Sync>;

/// One registry row: a **static** descriptor (queried before instantiation) plus
/// the factory that builds the handle when selected.
#[derive(Clone)]
pub struct Entry<H> {
    pub descriptor: CapabilityDescriptor,
    pub factory: Factory<H>,
}

impl<H> Entry<H> {
    pub fn new<F>(descriptor: CapabilityDescriptor, factory: F) -> Self
    where
        F: Fn(&ProviderConfig) -> Result<H, ProtocolError> + Send + Sync + 'static,
    {
        Self {
            descriptor,
            factory: Arc::new(factory),
        }
    }

    /// Instantiate the handle for `config`.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn instantiate(&self, config: &ProviderConfig) -> Result<H, ProtocolError> {
        (self.factory)(config)
    }
}

impl<H> std::fmt::Debug for Entry<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}
