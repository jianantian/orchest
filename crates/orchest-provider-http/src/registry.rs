//! Provider registry over [`ProviderEntry`].
//!
//! After ADR-0002 Phase 3 the legacy `ProviderFactory` trait and per-provider
//! `*Factory` structs are gone: construction goes through the protocol factories
//! ([`ChatProtocolFactory`](crate::protocol::ChatProtocolFactory) /
//! [`MessagesProtocolFactory`](crate::protocol::MessagesProtocolFactory)) selected
//! from the resolved [`ProviderEntry`]. This registry is now just an enumerable
//! view over the built-in entries (identity + protocols + aliases + path
//! overrides + headers + profiles) used for provider-name validation and
//! diagnostics.

use crate::protocol::{all_provider_entries, ProviderEntry};

/// An enumerable registry of built-in provider entries.
pub struct ProviderRegistry {
    entries: &'static [&'static ProviderEntry],
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            entries: all_provider_entries(),
        }
    }

    /// The entry for `provider`, or `None` if unknown.
    pub fn get(&self, provider: &str) -> Option<&'static ProviderEntry> {
        self.entries.iter().copied().find(|e| e.name == provider)
    }

    /// Sorted list of supported provider names.
    pub fn supported_providers(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.entries.iter().map(|e| e.name).collect();
        names.sort_unstable();
        names
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
