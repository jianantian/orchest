//! AgentRef: typed pub(crate) API wrapping ActorRef<AgentMsg>.

use ractor::ActorRef;

use super::actor::{AgentMsg, CancelCmd, InjectCmd, SteerCmd};

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // v0.8 forward declaration
pub(crate) enum AgentError {
    #[error("communication error: {0}")]
    Communication(String),
}

/// Typed handle for interacting with a running agent actor.
#[allow(dead_code)] // v0.8 forward declaration
pub(crate) struct AgentRef {
    pub(crate) inner: ActorRef<AgentMsg>,
}

#[allow(dead_code)] // justified: pub(crate) API used by RunHandle/supervisor, not all methods have callers yet
impl AgentRef {
    /// Inject a user-role message into the running agent (fire-and-forget).
    pub(crate) fn inject(&self, cmd: InjectCmd) {
        let _ = self.inner.cast(AgentMsg::Inject(cmd));
    }

    /// Inject a system-role steering instruction (fire-and-forget).
    pub(crate) fn steer(&self, cmd: SteerCmd) {
        let _ = self.inner.cast(AgentMsg::Steer(cmd));
    }

    /// Cancel the running agent.
    pub(crate) fn cancel(&self) {
        let _ = self
            .inner
            .cast(AgentMsg::Cancel(CancelCmd { reason: None }));
    }
}
