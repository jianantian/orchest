//! Global shared reqwest::Client for provider adapters.

use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn shared_client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .timeout(Duration::from_secs(300))
            .build()
            .expect("failed to build shared reqwest::Client")
    })
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
}
