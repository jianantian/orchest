//! Shared `reqwest::Client` for AIGC providers.
//!
//! Construction is delegated to `orchest_provider_core::http` (v0.9.12 — the one
//! shared client builder). This crate keeps its own cached instance with its
//! historic timeouts (300s, 20 idle conns/host) so behavior is unchanged.

use std::sync::OnceLock;

use orchest_provider_core::http::{cached, HttpClientConfig};

pub fn shared_client() -> &'static reqwest::Client {
    static SHARED_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    cached(&SHARED_CLIENT, HttpClientConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_client_is_singleton() {
        let a = shared_client() as *const reqwest::Client;
        let b = shared_client() as *const reqwest::Client;
        assert_eq!(a, b);
    }
}
