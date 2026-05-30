//! AgentRef: typed pub(crate) API wrapping ActorRef<AgentMsg>.
//!
//! v0.7: definition and method signatures are in place; full logic is wired
//! for cancel(). steer() is a stub (returns immediately) — implementation
//! ships in v0.8 when callers exist.

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
    /// Request a steering redirect (v0.8 implementation).
    pub(crate) async fn steer(&self, _cmd: SteerCmd) -> Result<SteerResult, AgentError> {
        ractor::call!(self.inner, AgentMsg::Steer, _cmd)
            .map_err(|e| AgentError::Communication(e.to_string()))
    }

    /// Inject a message into the running agent (v0.8 implementation).
    pub(crate) async fn inject(&self, _cmd: InjectCmd) -> Result<(), AgentError> {
        ractor::call!(self.inner, AgentMsg::Inject, _cmd)
            .map_err(|e| AgentError::Communication(e.to_string()))
    }

    /// Cancel the running agent.
    pub(crate) fn cancel(&self) {
        let _ = self.inner.cast(AgentMsg::Cancel(CancelCmd));
    }
}
