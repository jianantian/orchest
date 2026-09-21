//! OSS upload + gen-task poller (behind the `oss` feature — pulls signing crates).
//!
//! Signed/polled generation (volc-visual, aliyun) submits a job then polls until
//! it completes. This module provides the reusable **poll loop**; the per-dialect
//! submit/fetch and OSS object signing land in `orchest-provider-visual`
//! (Issue 007). Auth signing primitives live in [`crate::auth::hmac_signer`].

use std::time::Duration;

use orchest_protocol::ProtocolError;

use crate::retry::RetryPolicy;

/// Outcome of one poll of a generation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollOutcome {
    /// Still running — poll again after the backoff.
    Pending,
    /// Terminal success.
    Done,
    /// Terminal failure.
    Failed,
}

/// Poll `check` until it reports a terminal state or the attempt budget is
/// exhausted, sleeping `policy.delay_for(attempt)` between polls. The shared loop
/// under every signed/polled gen dialect.
#[allow(clippy::result_large_err)] // ProtocolError carries several Strings; threshold is 128B
pub async fn poll_until_done<F, Fut>(
    policy: &RetryPolicy,
    mut check: F,
) -> Result<PollOutcome, ProtocolError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<PollOutcome, ProtocolError>>,
{
    let mut attempt = 1;
    loop {
        match check().await? {
            PollOutcome::Pending if attempt < policy.max_attempts => {
                tokio::time::sleep(policy.delay_for(attempt)).await;
                attempt += 1;
            }
            PollOutcome::Pending => {
                return Err(ProtocolError::new(
                    orchest_protocol::ErrorCode::Timeout,
                    "generation job did not complete within the poll budget",
                ));
            }
            terminal => return Ok(terminal),
        }
    }
}

/// A polling budget tuned for asset generation (longer than the default retry):
/// up to ~2 minutes of polling.
pub fn gen_poll_policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 60,
        base_delay: Duration::from_secs(2),
        max_delay: Duration::from_secs(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test(start_paused = true)]
    async fn polls_until_done() {
        let calls = AtomicU32::new(0);
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
        };
        let out = poll_until_done(&policy, || async {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(if n < 3 {
                PollOutcome::Pending
            } else {
                PollOutcome::Done
            })
        })
        .await
        .unwrap();
        assert_eq!(out, PollOutcome::Done);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn times_out_when_never_done() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
        };
        let out = poll_until_done(&policy, || async { Ok(PollOutcome::Pending) }).await;
        assert!(matches!(
            out,
            Err(e) if e.code == orchest_protocol::ErrorCode::Timeout
        ));
    }
}
