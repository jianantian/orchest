//! `orchest-provider` — THE WALL (v0.9.12 provider unification, Issue 004).
//!
//! Consumers depend on exactly `orchest-protocol` + `orchest-provider` and
//! select providers by **capability query** or **identity pick** through one
//! mechanism; impl crates and wire dialects are never named. `features` control
//! compiled weight (`llm` pulls only the REST tier — no websocket/OSS).
//!
//! ```
//! use orchest_provider::Registry;
//! use orchest_protocol::Modality;
//!
//! let reg = Registry::with_builtin();         // whatever features enable
//! // list-then-choose (capability-only select is ambiguous across providers):
//! let candidates = reg.chat().accepts([Modality::Text, Modality::Image]).thinking().list();
//! let _ = candidates.first();
//! // capability + identity mixed:
//! let _ = reg.asr().provider("volcengine").bidirectional().select();
//! // identity pick:
//! let _ = reg.chat().id("openai/gpt-5.4").select();
//! ```
//!
//! The exact selection surface (PRD Decision 4) is recorded in
//! `docs/archive/iteration/v0_9_12/issues/004-registry-wall/selection-api.md`.
//!
//! The `testing` feature adds [`fakes`]: deterministic in-process
//! `Asr`/`Tts` implementations for downstream crates that need offline
//! tests or demos without hand-writing their own fake against
//! `orchest_protocol`. It pulls no impl crate and no network deps, and is
//! never enabled by default or by any other feature.

pub mod catalog;
pub mod facade;
#[cfg(feature = "testing")]
pub mod fakes;
pub mod registry;

pub use registry::{Query, Registry};

// Credential-free multi-capability catalog discovery (Batch 0).
pub use catalog::{find_model, find_model_for, list_models, ModelFilter};
pub use orchest_provider_core::catalog::{CatalogExt, ModelRecord, ModelStatus};

// Re-export the registry building blocks so impl crates and consumers share one
// set of entry/config types.
pub use orchest_provider_core::registry::{Entry, Factory, ProviderConfig};

/// Free-function construction of an LLM chat adapter from an explicit
/// `provider/model` + credentials, surfaced through the wall (feature `http`).
///
/// The [`Registry`] identity/capability picks cover every **enumerable** model,
/// but dynamic-gateway providers (OpenRouter) cannot be enumerated statically —
/// so `node`/`py` and the runtime examples construct through this path. Keeping
/// it here means consumers still depend only on `orchest-protocol` +
/// `orchest-provider`, never on an impl crate directly.
#[cfg(feature = "http")]
pub use orchest_provider_http::{
    create_adapter, create_adapter_from_config, normalize_provider_model, NormalizedProviderModel,
    ProviderRuntimeConfig,
};

/// The vendor-namespaced facade (`orchest_provider::providers::volcengine::…`).
pub mod providers {
    pub use crate::facade::*;
}
