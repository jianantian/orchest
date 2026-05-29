use std::sync::OnceLock;
use std::time::Duration;

static SHARED_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn shared_client() -> &'static reqwest::Client {
    // INVARIANT: reqwest::Client::builder() with these fixed parameters can
    // only fail on catastrophic platform misconfiguration (missing TLS
    // backend, exhausted file descriptors at startup).  An early panic is
    // preferable to silently failing on every subsequent HTTP request.
    SHARED_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .timeout(Duration::from_secs(300))
            .build()
            .expect("reqwest client configuration should be valid")
    })
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
