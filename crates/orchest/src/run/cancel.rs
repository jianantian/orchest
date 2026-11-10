//! Run cancellation: a cloneable token that carries why a run was stopped.
//!
//! [`RunHandle::cancel`](crate::run::RunHandle::cancel) sets the token at
//! once; it does not wait for the current step to finish. The run observes it
//! while waiting on the model, on an approval, on a tool it may interrupt, and
//! before every tool dispatch. A tool that may not be interrupted runs to
//! completion and its result is recorded; after that no new model call or
//! tool call starts. A delegated child run holds a child token, so cancelling
//! a parent cancels its children (with [`CancelReason::Upstream`]).

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// Why a run was cancelled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CancelReason {
    /// A user asked to stop.
    #[default]
    User,
    /// A deadline the host enforces ran out.
    Timeout,
    /// The parent run (or another upstream owner) was cancelled.
    Upstream,
    /// The host lost the connection to whoever was driving the run.
    HostDisconnected,
    /// A watcher decided to abort the run.
    Watcher,
}

/// A cancellation request: the reason and an optional message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cancellation {
    pub reason: CancelReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Cancellation {
    pub fn new(reason: CancelReason) -> Self {
        Self {
            reason,
            message: None,
        }
    }

    pub fn user() -> Self {
        Self::new(CancelReason::User)
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Shared cancellation state of one run. Cloning shares it; cancelling is
/// idempotent and the first cancellation's reason wins.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    token: CancellationToken,
    cause: Arc<OnceLock<Cancellation>>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels the run. Returns `true` for the call that cancelled it,
    /// `false` when it was already cancelled.
    pub fn cancel(&self, cancellation: Cancellation) -> bool {
        let first = self.cause.set(cancellation).is_ok();
        self.token.cancel();
        first
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Why the run was cancelled, once it is. A child cancelled through its
    /// parent reports [`CancelReason::Upstream`].
    pub fn cancellation(&self) -> Option<Cancellation> {
        if !self.token.is_cancelled() {
            return None;
        }
        Some(
            self.cause
                .get()
                .cloned()
                .unwrap_or_else(|| Cancellation::new(CancelReason::Upstream)),
        )
    }

    /// Completes when the run is cancelled.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// A token cancelled with this one (but not the other way round), for a
    /// delegated child run.
    pub fn child(&self) -> Self {
        Self {
            token: self.token.child_token(),
            cause: Arc::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_is_idempotent_and_first_reason_wins() {
        let token = CancelToken::new();
        assert!(token.cancellation().is_none());
        assert!(token.cancel(Cancellation::new(CancelReason::Timeout)));
        assert!(!token.cancel(Cancellation::user()));
        token.cancelled().await;
        assert_eq!(token.cancellation().unwrap().reason, CancelReason::Timeout);
    }

    #[test]
    fn child_follows_parent_but_not_the_reverse() {
        let parent = CancelToken::new();
        let child = parent.child();
        child.cancel(Cancellation::user());
        assert!(!parent.is_cancelled());

        let other = parent.child();
        parent.cancel(Cancellation::user().with_message("stop"));
        assert!(other.is_cancelled());
        assert_eq!(other.cancellation().unwrap().reason, CancelReason::Upstream);
        assert_eq!(child.cancellation().unwrap().reason, CancelReason::User);
    }
}
