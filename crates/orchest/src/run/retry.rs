//! LLM retry policy: error classification, backoff strategy, retry helper.

use std::time::Duration;

use crate::model::ModelError;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: BackoffStrategy,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BackoffStrategy {
    Fixed(Duration),
    Exponential {
        base: Duration,
        max: Duration,
        /// Add ±25 % random jitter to prevent thundering herd.
        jitter: bool,
    },
}

impl RetryPolicy {
    /// Recommended one-line retry policy for interactive agents.
    ///
    /// Covers the transient error classes recognized by the runtime:
    ///
    /// - **429 rate limits** (honours the provider's `Retry-After` when present)
    /// - **5xx server errors**
    /// - **timeouts** (`ModelError::code == "timeout"`)
    /// - **network-layer stream interrupts** (`code == "stream_error"` or
    ///   `"stream_interrupted"` — an SSE stream that dies mid-response or ends
    ///   without its completion signal). Retrying these is safe: the retry
    ///   re-issues a single `ModelAdapter::complete()` call, and partial stream
    ///   events forwarded before the failure are never committed to run state.
    ///
    /// Protocol-level errors — malformed SSE payloads (`invalid_json`),
    /// undecodable tool arguments (`invalid_tool_arguments`) — indicate a
    /// provider bug and are never retried, under this or any other policy.
    ///
    /// Defaults: 3 retries (4 attempts total), exponential backoff starting at
    /// 1 s, capped at 30 s, with ±25 % jitter.
    ///
    /// ```
    /// use orchest::run::RetryPolicy;
    /// let policy = RetryPolicy::recommended();
    /// assert_eq!(policy.max_retries, 3);
    /// ```
    pub fn recommended() -> Self {
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

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::recommended()
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum RetryClass {
    RateLimit,
    ServerError,
    Timeout,
    /// Network-layer stream interrupt: SSE chunk read failure or a stream that
    /// ended without its completion signal. Distinct from protocol-level parse
    /// errors, which stay `NoRetry`.
    StreamInterrupted,
    NoRetry,
}

pub(crate) fn classify(error: &ModelError) -> RetryClass {
    match error.status {
        Some(429) => RetryClass::RateLimit,
        Some(s) if s >= 500 => RetryClass::ServerError,
        _ => match error.code.as_deref() {
            Some("timeout") => RetryClass::Timeout,
            Some("stream_error" | "stream_interrupted") => RetryClass::StreamInterrupted,
            _ => RetryClass::NoRetry,
        },
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
        RetryClass::RateLimit
            | RetryClass::ServerError
            | RetryClass::Timeout
            | RetryClass::StreamInterrupted
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

    fn err_with_code(code: &str) -> ModelError {
        ModelError {
            message: "err".into(),
            code: Some(code.into()),
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
    fn classify_timeout_code_is_timeout() {
        assert_eq!(classify(&err_with_code("timeout")), RetryClass::Timeout);
    }

    #[test]
    fn classify_stream_error_is_stream_interrupted() {
        assert_eq!(
            classify(&err_with_code("stream_error")),
            RetryClass::StreamInterrupted
        );
    }

    #[test]
    fn classify_stream_interrupted_is_stream_interrupted() {
        assert_eq!(
            classify(&err_with_code("stream_interrupted")),
            RetryClass::StreamInterrupted
        );
    }

    #[test]
    fn classify_protocol_errors_no_retry() {
        // Malformed SSE payloads / tool arguments are provider bugs, not
        // transient network failures — retrying would replay the same bug.
        assert_eq!(
            classify(&err_with_code("invalid_json")),
            RetryClass::NoRetry
        );
        assert_eq!(
            classify(&err_with_code("invalid_tool_arguments")),
            RetryClass::NoRetry
        );
    }

    #[test]
    fn should_retry_returns_true_for_stream_interrupted_under_limit() {
        let policy = RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(0)),
        };
        assert!(should_retry(
            &RetryClass::StreamInterrupted,
            0,
            &Some(policy)
        ));
    }

    #[test]
    fn recommended_covers_all_transient_classes() {
        let policy = Some(RetryPolicy::recommended());
        for class in [
            RetryClass::RateLimit,
            RetryClass::ServerError,
            RetryClass::Timeout,
            RetryClass::StreamInterrupted,
        ] {
            assert!(
                should_retry(&class, 0, &policy),
                "recommended policy should retry {class:?}"
            );
        }
        assert!(!should_retry(&RetryClass::NoRetry, 0, &policy));
        assert_eq!(RetryPolicy::default().max_retries, 3);
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
