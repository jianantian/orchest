// Webhook server for async job completion callbacks.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

use crate::tool::async_job::JobStatus;

pub(crate) struct WebhookRuntime {
    pub(crate) base_url: String,
    pub(crate) waiters: Arc<Mutex<HashMap<String, oneshot::Sender<JobStatus>>>>,
    abort_handle: tokio::task::AbortHandle,
}

impl Drop for WebhookRuntime {
    fn drop(&mut self) {
        self.abort_handle.abort();
    }
}

pub(crate) async fn start_webhook_server() -> Result<WebhookRuntime, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind webhook server: {e}"))?;
    let address = listener
        .local_addr()
        .map_err(|e| format!("failed to read webhook address: {e}"))?;
    let waiters: Arc<Mutex<HashMap<String, oneshot::Sender<JobStatus>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let waiters_for_task = Arc::clone(&waiters);

    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let waiters = Arc::clone(&waiters_for_task);
            tokio::spawn(async move {
                let mut buffer = vec![0; 16 * 1024];
                let mut read_total = 0usize;
                loop {
                    let Ok(read) = socket.read(&mut buffer[read_total..]).await else {
                        return;
                    };
                    if read == 0 {
                        break;
                    }
                    read_total += read;
                    let request = String::from_utf8_lossy(&buffer[..read_total]);
                    if let Some(header_end) =
                        request.find("\r\n\r\n").or_else(|| request.find("\n\n"))
                    {
                        let header = &request[..header_end];
                        let body_start = if request[header_end..].starts_with("\r\n\r\n") {
                            header_end + 4
                        } else {
                            header_end + 2
                        };
                        let content_length = header
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        if read_total >= body_start + content_length {
                            break;
                        }
                    }
                    if read_total == buffer.len() {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&buffer[..read_total]);
                let Some(first_line) = request.lines().next() else {
                    return;
                };
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() < 2 || parts[0] != "POST" {
                    let _ = write_http_response(&mut socket, 405, "method not allowed").await;
                    return;
                }
                let Some(job_id) = parts[1].strip_prefix("/webhooks/async-job/") else {
                    let _ = write_http_response(&mut socket, 404, "not found").await;
                    return;
                };
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .or_else(|| request.split("\n\n").nth(1))
                    .unwrap_or_default();
                let parsed: Value = match serde_json::from_str(body) {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = write_http_response(&mut socket, 400, "bad request").await;
                        return;
                    }
                };
                let status = match parsed.get("status").and_then(Value::as_str) {
                    Some("completed") => {
                        JobStatus::Completed(parsed.get("result").cloned().unwrap_or(Value::Null))
                    }
                    Some("failed") => JobStatus::Failed(
                        parsed
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("webhook job failed")
                            .to_string(),
                    ),
                    _ => JobStatus::Pending {
                        progress: parsed
                            .get("progress")
                            .and_then(Value::as_f64)
                            .map(|value| value as f32),
                        message: parsed
                            .get("message")
                            .and_then(Value::as_str)
                            .map(String::from),
                    },
                };
                if let Some(waiter) = waiters.lock().await.remove(job_id) {
                    let _ = waiter.send(status);
                }
                let _ = write_http_response(&mut socket, 200, "ok").await;
            });
        }
    });

    Ok(WebhookRuntime {
        base_url: format!("http://{address}"),
        waiters,
        abort_handle: task.abort_handle(),
    })
}

async fn write_http_response(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    socket
        .write_all(
            format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
}
