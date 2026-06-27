//! L0: the one shared `reqwest::Client` builder.
//!
//! Collapses the near-identical per-crate `http.rs` (`providers`, `asr`, `aigc`)
//! into a single configurable construction. Each crate keeps its own cached
//! client with its own timeouts via [`HttpClientConfig`] — the *construction*
//! is unified, the *parameters* stay per-crate so no request timeout changes.

use std::sync::OnceLock;
use std::time::Duration;

/// Tunables for the shared client. Defaults match the historic
/// `agent-runtime-providers` client (300s timeout, 20 idle conns/host).
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub connect_timeout: Option<Duration>,
    pub timeout: Option<Duration>,
    pub pool_max_idle_per_host: Option<usize>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: None,
            timeout: Some(Duration::from_secs(300)),
            pool_max_idle_per_host: Some(20),
        }
    }
}

impl HttpClientConfig {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = Some(d);
        self
    }

    #[must_use]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    #[must_use]
    pub fn pool_max_idle_per_host(mut self, n: usize) -> Self {
        self.pool_max_idle_per_host = Some(n);
        self
    }
}

/// Build a `reqwest::Client` from `config`.
///
/// # Panics
/// Panics only if the client cannot be constructed, which can happen only on
/// catastrophic platform misconfiguration (missing TLS backend, FD exhaustion
/// at startup). An early panic is preferable to silently failing on every
/// subsequent request — matching the historic per-crate behavior.
pub fn build_client(config: &HttpClientConfig) -> reqwest::Client {
    let mut builder = reqwest::Client::builder();
    if let Some(d) = config.connect_timeout {
        builder = builder.connect_timeout(d);
    }
    if let Some(d) = config.timeout {
        builder = builder.timeout(d);
    }
    if let Some(n) = config.pool_max_idle_per_host {
        builder = builder.pool_max_idle_per_host(n);
    }
    builder
        .build()
        .expect("failed to build shared reqwest::Client")
}

/// Get-or-init a cached client in the caller's `OnceLock` using `config`.
/// The idiom for a per-crate `shared_client()` that delegates construction here.
pub fn cached(
    slot: &'static OnceLock<reqwest::Client>,
    config: HttpClientConfig,
) -> &'static reqwest::Client {
    slot.get_or_init(|| build_client(&config))
}

/// The default shared client (300s timeout, 20 idle conns/host).
pub fn shared_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    cached(&CLIENT, HttpClientConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_client_is_singleton() {
        let a = shared_client() as *const _;
        let b = shared_client() as *const _;
        assert_eq!(a, b);
    }

    #[test]
    fn builds_with_custom_config() {
        let c = build_client(
            &HttpClientConfig::new()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(120)),
        );
        // smoke: the client exists and is usable as a value
        let _ = c;
    }
}
