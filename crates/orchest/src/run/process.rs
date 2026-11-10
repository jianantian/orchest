//! Process termination backend: an injection point for stopping external
//! processes a run started when the run is cancelled.
//!
//! Orchest does not implement process-group termination itself (it is
//! platform specific and belongs to whoever spawns the processes). A host
//! injects a [`ProcessTerminator`] with
//! [`AgentConfigBuilder::process_terminator`](crate::run::AgentConfigBuilder::process_terminator);
//! on cancellation the run calls it once, best effort, and reports the
//! outcome as `RuntimeEvent::ProcessesTerminated`, failures included.

use std::time::Duration;

use async_trait::async_trait;

use super::cancel::Cancellation;
use super::config::RunId;

/// How long a run waits for the terminator before reporting it as failed.
pub const PROCESS_TERMINATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Stops the external processes a run started.
#[async_trait]
pub trait ProcessTerminator: Send + Sync {
    /// Terminates what `run_id` still has running. Returns how many processes
    /// were signalled, or why termination failed.
    async fn terminate(&self, run_id: RunId, cancellation: &Cancellation) -> Result<usize, String>;
}
