//! Webhook server for async job completion callbacks.

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

                // Read until we have the full request (headers + body).
                loop {
                    let Ok(read) = socket.read(&mut buffer[read_total..]).await else {
                        return;
                    };
                    if read == 0 {
                        break;
                    }
                    read_total += read;
                    if let Some(body_offset) = header_body_split(&buffer[..read_total]) {
                        let content_length = parse_content_length(&buffer[..read_total]);
                        if read_total >= body_offset + content_length {
                            break;
                        }
                    }
                    if read_total == buffer.len() {
                        break;
                    }
                }

                let mut headers = [httparse::EMPTY_HEADER; 32];
                let mut req = httparse::Request::new(&mut headers);
                let body_offset = match req.parse(&buffer[..read_total]) {
                    Ok(httparse::Status::Complete(offset)) => offset,
                    _ => return,
                };

                if req.method != Some("POST") {
                    let _ = write_http_response(&mut socket, 405, "method not allowed").await;
                    return;
                }
                let path = req.path.unwrap_or("");
                let Some(job_id) = path.strip_prefix("/webhooks/async-job/") else {
                    let _ = write_http_response(&mut socket, 404, "not found").await;
                    return;
                };

                let body = &buffer[body_offset..read_total];
                let parsed: Value = match serde_json::from_slice(body) {
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

/// Find the byte offset where the HTTP body begins (after \r\n\r\n).
fn header_body_split(data: &[u8]) -> Option<usize> {
    data.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

/// Parse Content-Length from raw HTTP bytes using httparse.
fn parse_content_length(data: &[u8]) -> usize {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers);
    if req.parse(data).is_ok() {
        for header in req.headers.iter() {
            if header.name.eq_ignore_ascii_case("content-length") {
                if let Ok(s) = std::str::from_utf8(header.value) {
                    return s.trim().parse().unwrap_or(0);
                }
            }
        }
    }
    0
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    #[test]
    fn header_body_split_finds_boundary() {
        let data = b"POST /path HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        assert_eq!(header_body_split(data), Some(42));
    }

    #[test]
    fn header_body_split_none_when_incomplete() {
        let data = b"POST /path HTTP/1.1\r\nContent-Length: 5\r\n";
        assert_eq!(header_body_split(data), None);
    }

    #[test]
    fn parse_content_length_extracts_value() {
        let data = b"POST / HTTP/1.1\r\nContent-Length: 42\r\n\r\n";
        assert_eq!(parse_content_length(data), 42);
    }

    #[test]
    fn parse_content_length_case_insensitive() {
        let data = b"POST / HTTP/1.1\r\ncontent-length: 7\r\n\r\n";
        assert_eq!(parse_content_length(data), 7);
    }

    #[test]
    fn parse_content_length_missing_returns_zero() {
        let data = b"POST / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(parse_content_length(data), 0);
    }

    #[tokio::test]
    async fn webhook_completes_job_on_valid_post() {
        let runtime = start_webhook_server().await.expect("start webhook");
        let (tx, rx) = oneshot::channel();
        runtime
            .waiters
            .lock()
            .await
            .insert("job-123".to_string(), tx);

        let url = format!("{}/webhooks/async-job/job-123", runtime.base_url);
        let addr = url.strip_prefix("http://").unwrap();
        let host_port = addr.split('/').next().unwrap();
        let path = &addr[host_port.len()..];

        let body = r#"{"status":"completed","result":{"answer":42}}"#;
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(host_port).await.expect("connect");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");

        let status = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
            .await
            .expect("timeout")
            .expect("receive status");

        match status {
            JobStatus::Completed(value) => {
                assert_eq!(value["answer"], 42);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn webhook_returns_400_on_invalid_json() {
        let runtime = start_webhook_server().await.expect("start webhook");
        let addr = runtime.base_url.strip_prefix("http://").unwrap();

        let body = "not json at all";
        let request = format!(
            "POST /webhooks/async-job/job-bad HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");

        let mut response = vec![0u8; 1024];
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            stream.read(&mut response),
        )
        .await
        .expect("timeout")
        .expect("read response");
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("400"),
            "expected 400 in response: {response_str}"
        );
    }

    #[tokio::test]
    async fn webhook_returns_405_on_get() {
        let runtime = start_webhook_server().await.expect("start webhook");
        let addr = runtime.base_url.strip_prefix("http://").unwrap();

        let request = format!("GET /webhooks/async-job/job-get HTTP/1.1\r\nHost: {addr}\r\n\r\n");
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");

        let mut response = vec![0u8; 1024];
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            stream.read(&mut response),
        )
        .await
        .expect("timeout")
        .expect("read response");
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("405"),
            "expected 405 in response: {response_str}"
        );
    }
}
