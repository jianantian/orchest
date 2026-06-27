//! `orchest-providers` — THE WALL (v0.9.12 provider unification, Issue 004).
//!
//! Consumers depend on exactly `orchest-protocol` + `orchest-providers` and
//! select providers by **capability query** or **identity pick** through one
//! mechanism; impl crates and wire dialects are never named. `features` control
//! compiled weight (`llm` pulls only the REST tier — no websocket/OSS).
//!
//! ```
//! use orchest_providers::Registry;
//! use orchest_protocol::Modality;
//!
//! let reg = Registry::with_builtin();         // whatever features enable
//! // capability query:
//! let _ = reg.chat().accepts([Modality::Text, Modality::Image]).thinking().select();
//! // capability + identity mixed:
//! let _ = reg.asr().provider("volcengine").bidirectional().select();
//! // identity pick:
//! let _ = reg.chat().id("openai/gpt-5.4").select();
//! ```
//!
//! The exact selection surface (PRD Decision 4) is recorded in
//! `docs/iteration/v0_9_12/issues/004-registry-wall/selection-api.md`.

pub mod facade;
pub mod registry;

pub use registry::{Query, Registry};

// Re-export the registry building blocks so impl crates and consumers share one
// set of entry/config types.
pub use orchest_provider_core::registry::{Entry, Factory, ProviderConfig};

/// The vendor-namespaced facade (`orchest_providers::providers::volcengine::…`).
pub mod providers {
    pub use crate::facade::*;
}
