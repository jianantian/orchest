use std::sync::OnceLock;
use std::time::Duration;

static SHARED_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn shared_client() -> &'static reqwest::Client {
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
