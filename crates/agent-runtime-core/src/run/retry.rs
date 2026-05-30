//! LLM retry policy: error classification, backoff strategy, retry helper.

use std::time::Duration;

use crate::model::ModelError;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: BackoffStrategy,
}

#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    Fixed(Duration),
    Exponential {
        base: Duration,
        max: Duration,
        /// Add ±25 % random jitter to prevent thundering herd.
        jitter: bool,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
                jitter: true,
            },
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum RetryClass {
    RateLimit,
    ServerError,
    Timeout,
    NoRetry,
}

pub(crate) fn classify(error: &ModelError) -> RetryClass {
    match error.status {
        Some(429) => RetryClass::RateLimit,
        Some(s) if s >= 500 => RetryClass::ServerError,
        _ => {
            if error.code.as_deref() == Some("timeout") {
                RetryClass::Timeout
            } else {
                RetryClass::NoRetry
            }
        }
    }
}

/// Compute the delay for a retry attempt.
///
/// For rate-limit errors, honours `ModelError::retry_after_secs` if present.
pub(crate) fn compute_delay(attempt: u32, error: &ModelError, policy: &RetryPolicy) -> Duration {
    // Honour Retry-After header for 429 errors.
    if error.status == Some(429) {
        if let Some(secs) = error.retry_after_secs {
            return Duration::from_secs(secs);
        }
    }

    match &policy.backoff {
        BackoffStrategy::Fixed(d) => *d,
        BackoffStrategy::Exponential { base, max, jitter } => {
            let exp = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
            let millis = base.as_millis() as u64;
            let raw = millis.saturating_mul(exp);
            let capped = raw.min(max.as_millis() as u64);
            let final_millis = if *jitter {
                apply_jitter(capped)
            } else {
                capped
            };
            Duration::from_millis(final_millis)
        }
    }
}

fn apply_jitter(millis: u64) -> u64 {
    // ±25 % uniform jitter
    if millis == 0 {
        return 0;
    }
    let quarter = millis / 4;
    // Use a simple LCG seeded from the current time for no-dep randomness.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64;
    let noise = seed % (quarter * 2 + 1);
    millis.saturating_sub(quarter) + noise
}

/// Returns whether the error should be retried given the current attempt count and policy.
pub(crate) fn should_retry(class: &RetryClass, attempt: u32, policy: &Option<RetryPolicy>) -> bool {
    let Some(policy) = policy else { return false };
    if attempt >= policy.max_retries {
        return false;
    }
    matches!(
        class,
        RetryClass::RateLimit | RetryClass::ServerError | RetryClass::Timeout
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_with_status(status: u16) -> ModelError {
        ModelError {
            message: "err".into(),
            code: None,
            provider: None,
            status: Some(status),
            retry_after_secs: None,
            upstream: None,
        }
    }

    fn err_no_status() -> ModelError {
        ModelError {
            message: "err".into(),
            code: None,
            provider: None,
            status: None,
            retry_after_secs: None,
            upstream: None,
        }
    }

    #[test]
    fn classify_429_is_rate_limit() {
        assert_eq!(classify(&err_with_status(429)), RetryClass::RateLimit);
    }

    #[test]
    fn classify_500_is_server_error() {
        assert_eq!(classify(&err_with_status(500)), RetryClass::ServerError);
    }

    #[test]
    fn classify_503_is_server_error() {
        assert_eq!(classify(&err_with_status(503)), RetryClass::ServerError);
    }

    #[test]
    fn classify_400_no_retry() {
        assert_eq!(classify(&err_with_status(400)), RetryClass::NoRetry);
    }

    #[test]
    fn classify_401_no_retry() {
        assert_eq!(classify(&err_with_status(401)), RetryClass::NoRetry);
    }

    #[test]
    fn classify_unknown_no_retry() {
        assert_eq!(classify(&err_no_status()), RetryClass::NoRetry);
    }

    #[test]
    fn should_retry_returns_false_when_no_policy() {
        assert!(!should_retry(&RetryClass::RateLimit, 0, &None));
    }

    #[test]
    fn should_retry_returns_false_when_max_retries_reached() {
        let policy = RetryPolicy {
            max_retries: 2,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(0)),
        };
        assert!(!should_retry(&RetryClass::RateLimit, 2, &Some(policy)));
    }

    #[test]
    fn should_retry_returns_true_for_rate_limit_under_limit() {
        let policy = RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(0)),
        };
        assert!(should_retry(&RetryClass::RateLimit, 0, &Some(policy)));
    }

    #[test]
    fn compute_delay_fixed() {
        let policy = RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(500)),
        };
        assert_eq!(
            compute_delay(0, &err_no_status(), &policy),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn compute_delay_honours_retry_after() {
        let mut err = err_with_status(429);
        err.retry_after_secs = Some(10);
        let policy = RetryPolicy::default();
        assert_eq!(compute_delay(0, &err, &policy), Duration::from_secs(10));
    }

    #[test]
    fn compute_delay_exponential_without_jitter_grows() {
        let policy = RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_millis(100),
                max: Duration::from_secs(60),
                jitter: false,
            },
        };
        let d0 = compute_delay(0, &err_no_status(), &policy);
        let d1 = compute_delay(1, &err_no_status(), &policy);
        assert!(d1 > d0, "delay should grow with attempt count");
    }
}
