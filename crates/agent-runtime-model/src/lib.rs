//! Deprecated re-export shell for the former `agent-runtime-model` crate.
//!
//! The protocol spine moved to [`orchest_protocol`] in v0.9.12 (provider
//! unification). This crate now only re-exports it so existing
//! `agent_runtime_model::*` paths keep resolving. New code should depend on
//! `orchest-protocol` directly. Scheduled for removal in Issue 008.

pub use orchest_protocol::*;
