//! Tool execution helpers: sync dispatch, async job polling, and webhook waits.

use std::time::Instant;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::tool::async_job::{JobHandle, JobStatus};
use crate::tool::ToolError;

use super::helpers::emit;
use super::webhook::WebhookRuntime;

pub(crate) fn tool_error_result(error: &ToolError) -> Value {
    json!({ "error": error })
}

pub(crate) async fn poll_async_job(
    tx: &mpsc::Sender<RuntimeEvent>,
    tool_name: &str,
    handle: &JobHandle,
    start_time: Instant,
    webhook_runtime: &Option<WebhookRuntime>,
) -> Value {
    let timeout = handle.timeout;

    if let (Some(webhook), Some(runtime)) = (&handle.webhook, webhook_runtime) {
        let (webhook_tx, webhook_rx) = tokio::sync::oneshot::channel();
        runtime
            .waiters
            .lock()
            .await
            .insert(webhook.expected_job_id.clone(), webhook_tx);
        let wait_for = handle.poll_interval * 3;
        match tokio::time::timeout(wait_for, webhook_rx).await {
            Ok(Ok(JobStatus::Completed(value))) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolCompleted {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        output: value.clone(),
                        elapsed: start_time.elapsed(),
                    },
                )
                .await;
                return value;
            }
            Ok(Ok(JobStatus::Failed(error))) => {
                let error = ToolError::fatal(error.clone()).with_code("ASYNC_JOB_FAILED");
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: error.clone(),
                    },
                )
                .await;
                return tool_error_result(&error);
            }
            Ok(Ok(JobStatus::Pending { progress, message })) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolProgress {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        status: JobStatus::Pending { progress, message },
                    },
                )
                .await;
            }
            Ok(Err(_)) | Err(_) => {}
        }
        runtime
            .waiters
            .lock()
            .await
            .remove(&webhook.expected_job_id);
    }

    loop {
        tokio::time::sleep(handle.poll_interval).await;

        if let Some(max) = timeout {
            if start_time.elapsed() > max {
                let error = ToolError::transient("async job timed out").with_code("TIMEOUT");
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: error.clone(),
                    },
                )
                .await;
                return tool_error_result(&error);
            }
        }

        let Some(poll) = &handle.poll else {
            let error = ToolError::fatal("async job has no polling fallback").with_code("NO_POLL");
            emit(
                tx,
                RuntimeEvent::ToolCallFailed {
                    tool: tool_name.to_string(),
                    error: error.clone(),
                },
            )
            .await;
            return tool_error_result(&error);
        };

        match (poll)().await {
            Ok(JobStatus::Pending { progress, message }) => {
                emit(
                    tx,
                    RuntimeEvent::AsyncToolProgress {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        status: JobStatus::Pending { progress, message },
                    },
                )
                .await;
            }
            Ok(JobStatus::Completed(value)) => {
                let elapsed = start_time.elapsed();
                emit(
                    tx,
                    RuntimeEvent::AsyncToolCompleted {
                        tool: tool_name.to_string(),
                        job_id: handle.job_id.clone(),
                        output: value.clone(),
                        elapsed,
                    },
                )
                .await;
                return value;
            }
            Ok(JobStatus::Failed(err)) => {
                let error = ToolError::fatal(err.clone()).with_code("ASYNC_JOB_FAILED");
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: error.clone(),
                    },
                )
                .await;
                return tool_error_result(&error);
            }
            Err(e) => {
                emit(
                    tx,
                    RuntimeEvent::ToolCallFailed {
                        tool: tool_name.to_string(),
                        error: e.clone(),
                    },
                )
                .await;
                return tool_error_result(&e);
            }
        }
    }
}
