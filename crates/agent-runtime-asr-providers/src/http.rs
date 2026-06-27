//! Shared `reqwest::Client` for ASR providers.
//!
//! Construction is delegated to `orchest_provider_core::http` (v0.9.12 — the one
//! shared client builder). This crate keeps its own cached instance with its
//! historic timeouts (15s connect, 120s request) so behavior is unchanged.

use std::sync::OnceLock;
use std::time::Duration;

use orchest_provider_core::http::{cached, HttpClientConfig};

pub fn shared_client() -> &'static reqwest::Client {
    static SHARED_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    cached(
        &SHARED_CLIENT,
        HttpClientConfig {
            connect_timeout: Some(Duration::from_secs(15)),
            timeout: Some(Duration::from_secs(120)),
            pool_max_idle_per_host: None,
        },
    )
}
