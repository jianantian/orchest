use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ToolError;

pub type PollFn =
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<JobStatus, ToolError>> + Send>> + Send + Sync;

/// A handle to a long-running asynchronous tool job.
///
/// The `poll` closure cannot be serialized. When a `RunState` containing a
/// `JobHandle` is serialized and later deserialized, the async job state is
/// lost. Tool authors must handle idempotency when `execute()` is called again
/// after a cross-process restore.
pub struct JobHandle {
    pub job_id: String,
    pub poll: Arc<PollFn>,
    pub poll_interval: Duration,
    pub timeout: Option<Duration>,
}

impl std::fmt::Debug for JobHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobHandle")
            .field("job_id", &self.job_id)
            .field("poll_interval", &self.poll_interval)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Clone for JobHandle {
    fn clone(&self) -> Self {
        Self {
            job_id: self.job_id.clone(),
            poll: Arc::clone(&self.poll),
            poll_interval: self.poll_interval,
            timeout: self.timeout,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Pending {
        progress: Option<f32>,
        message: Option<String>,
    },
    Completed(Value),
    Failed(String),
}
