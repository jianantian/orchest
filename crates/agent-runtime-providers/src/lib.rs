//! **Deprecated re-export shell (v0.9.12 provider unification).**
//!
//! The LLM chat adapters and their construction API moved behind the wall into
//! [`orchest_provider_http`] (Issue 005). This crate now re-exports that surface
//! unchanged so existing consumers (`agent-runtime-core` tests, `node`, `py`,
//! the rust examples) keep compiling against `agent_runtime_providers::{
//! create_adapter_from_config, normalize_provider_model, ModelAdapter, ... }`.
//!
//! New code should depend on `orchest-provider-http` (impls) or
//! `orchest-providers` (the descriptor-queryable wall) directly. This shell is
//! removed in Issue 008.

pub use orchest_provider_http::*;
