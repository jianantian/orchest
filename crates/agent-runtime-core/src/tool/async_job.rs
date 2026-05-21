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
    pub poll: Option<Arc<PollFn>>,
    pub poll_interval: Duration,
    pub timeout: Option<Duration>,
    pub webhook: Option<WebhookConfig>,
}

impl Serialize for JobHandle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct SerializableJobHandle<'a> {
            job_id: &'a str,
            poll_interval: Duration,
            timeout: Option<Duration>,
            webhook: Option<&'a WebhookConfig>,
        }

        SerializableJobHandle {
            job_id: &self.job_id,
            poll_interval: self.poll_interval,
            timeout: self.timeout,
            webhook: self.webhook.as_ref(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JobHandle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializableJobHandle {
            job_id: String,
            poll_interval: Duration,
            timeout: Option<Duration>,
            webhook: Option<WebhookConfig>,
        }

        let value = SerializableJobHandle::deserialize(deserializer)?;
        Ok(Self {
            job_id: value.job_id,
            poll: None,
            poll_interval: value.poll_interval,
            timeout: value.timeout,
            webhook: value.webhook,
        })
    }
}

impl std::fmt::Debug for JobHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobHandle")
            .field("job_id", &self.job_id)
            .field("poll_interval", &self.poll_interval)
            .field("timeout", &self.timeout)
            .field("webhook", &self.webhook)
            .finish_non_exhaustive()
    }
}

impl Clone for JobHandle {
    fn clone(&self) -> Self {
        Self {
            job_id: self.job_id.clone(),
            poll: self.poll.as_ref().map(Arc::clone),
            poll_interval: self.poll_interval,
            timeout: self.timeout,
            webhook: self.webhook.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub expected_job_id: String,
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
