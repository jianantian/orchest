//! `orchest-provider-core` — L0 building blocks + L1 header-auth strategies for
//! the provider impl crates (v0.9.12 provider unification, Issue 003).
//!
//! Organized so dependency weight is isolated by feature:
//! - **L0 (always):** [`http`] client builder, [`retry`] backoff, [`telemetry`]
//!   primitives.
//! - **L0 (`sse`, default):** [`sse`] line decoder for REST/SSE dialects.
//! - **L0 (`ws`):** [`ws`] bidirectional websocket scaffold + binary-frame codec
//!   (pulls `tokio-tungstenite`).
//! - **L0 (`oss`):** [`oss`] gen-task poller + signing (pulls the crypto crates).
//! - **L1 (always):** [`auth`] header-injection strategies (`Bearer`, openspeech
//!   `X-Api-*`); the AK/SK-HMAC signer is behind `oss`.
//!
//! The default feature set pulls **no** `tokio-tungstenite` / OSS-signing deps,
//! so a pure-REST (LLM) consumer stays light.

pub mod auth;
pub mod gen;
pub mod http;
pub mod pricing;
pub mod registry;
pub mod retry;
pub mod telemetry;

#[cfg(feature = "sse")]
pub mod sse;

#[cfg(feature = "ws")]
pub mod ws;

#[cfg(feature = "oss")]
pub mod oss;

pub use auth::{BearerAuth, HeaderAuth, OpenSpeechHeaders};
pub use gen::{warn_unconsumed_params, SyncGenCache};
pub use http::{build_client, shared_client, HttpClientConfig};
pub use pricing::{Cost, Meter, Pricing};
pub use retry::RetryPolicy;
pub use telemetry::{new_trace_id, LatencyTimer};
