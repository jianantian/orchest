//! L0: retry with exponential backoff.

use std::time::Duration;

/// Exponential-backoff retry policy.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    /// Delay before attempt `attempt` (1-based): `base * 2^(attempt-1)`, capped.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(16);
        let scaled = self.base_delay.saturating_mul(1u32 << shift);
        scaled.min(self.max_delay)
    }

    /// Run `op`, retrying while `is_retryable(&err)` holds, up to `max_attempts`.
    /// Sleeps `delay_for(attempt)` between attempts.
    pub async fn run<T, E, F, Fut, R>(&self, mut op: F, is_retryable: R) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        R: Fn(&E) -> bool,
    {
        let mut attempt = 1;
        loop {
            match op().await {
                Ok(v) => return Ok(v),
                Err(e) if attempt < self.max_attempts && is_retryable(&e) => {
                    tokio::time::sleep(self.delay_for(attempt)).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn backoff_grows_and_caps() {
        let p = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
        };
        assert_eq!(p.delay_for(1), Duration::from_millis(100));
        assert_eq!(p.delay_for(2), Duration::from_millis(200));
        assert_eq!(p.delay_for(3), Duration::from_millis(400));
        assert_eq!(p.delay_for(20), Duration::from_secs(1)); // capped
    }

    #[tokio::test(start_paused = true)]
    async fn retries_until_success() {
        let calls = AtomicU32::new(0);
        let p = RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let out: Result<u32, &str> = p
            .run(
                || async {
                    let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    if n < 3 {
                        Err("transient")
                    } else {
                        Ok(n)
                    }
                },
                |_| true,
            )
            .await;
        assert_eq!(out, Ok(3));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn stops_on_non_retryable() {
        let calls = AtomicU32::new(0);
        let p = RetryPolicy::default();
        let out: Result<u32, &str> = p
            .run(
                || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err("fatal")
                },
                |_| false,
            )
            .await;
        assert_eq!(out, Err("fatal"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
