//! AgentRef: typed pub(crate) API wrapping ActorRef<AgentMsg>.

use ractor::ActorRef;

use super::actor::{AgentMsg, CancelCmd, InjectCmd, SteerCmd, SteerResult};

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

#[allow(dead_code)] // v0.8 forward declaration
impl AgentRef {
    /// Request a steering redirect (v0.9 implementation).
    pub(crate) async fn steer(&self, _cmd: SteerCmd) -> Result<SteerResult, AgentError> {
        ractor::call!(self.inner, AgentMsg::Steer, _cmd)
            .map_err(|e| AgentError::Communication(e.to_string()))
    }

    /// Inject a message into the running agent (fire-and-forget).
    pub(crate) fn inject(&self, cmd: InjectCmd) {
        let _ = self.inner.cast(AgentMsg::Inject(cmd));
    }

    /// Cancel the running agent.
    pub(crate) fn cancel(&self) {
        let _ = self
            .inner
            .cast(AgentMsg::Cancel(CancelCmd { reason: None }));
    }
}
