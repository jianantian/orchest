//! Vendor-namespaced facade scaffold (`providers::volcengine::{chat,asr,tts}`).
//!
//! The umbrella reconstructs a **vendor view** over implementations that
//! physically live in different dialect crates. Each vendor module is a thin
//! delegation to the registry filtered by `provider`, so a caller who wants
//! "Volcengine's chat" never names an impl crate or dialect.
//!
//! **Scaffold (Issue 004).** One representative vendor (`volcengine`) shows the
//! pattern + re-export shape; the other vendor modules (openai, anthropic,
//! minimax, …) are added alongside their registrations in Issues 005/006/007 by
//! copying this shape. With zero registered impls these return
//! `NoMatchingProvider`, which is the correct mechanism-only behavior.

use orchest_protocol::{Asr, ChatModel, ProtocolError, RealtimeSession, Tts};
use orchest_provider_core::registry::ProviderConfig;

use crate::registry::Registry;

/// Volcengine vendor view — the reference pattern for the facade. Volcengine is
/// the canonical "one vendor, three dialects" case (ark→chat REST, openspeech→
/// asr/tts/omni WS, visual→signed gen), so its construction spans impl crates,
/// yet the caller only ever says `providers::volcengine::asr(..)`.
pub mod volcengine {
    use super::*;

    const VENDOR: &str = "volcengine";

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn chat(
        reg: &Registry,
        config: ProviderConfig,
    ) -> Result<Box<dyn ChatModel>, ProtocolError> {
        reg.chat().provider(VENDOR).id(&config.model).build(&config)
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn asr(reg: &Registry, config: ProviderConfig) -> Result<Box<dyn Asr>, ProtocolError> {
        reg.asr().provider(VENDOR).id(&config.model).build(&config)
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn tts(reg: &Registry, config: ProviderConfig) -> Result<Box<dyn Tts>, ProtocolError> {
        reg.tts().provider(VENDOR).id(&config.model).build(&config)
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
    pub fn realtime(
        reg: &Registry,
        config: ProviderConfig,
    ) -> Result<Box<dyn RealtimeSession>, ProtocolError> {
        reg.realtime()
            .provider(VENDOR)
            .id(&config.model)
            .build(&config)
    }
}
