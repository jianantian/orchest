//! Loopback object-store server for signature-shape and status-mapping tests.
//!
//! Default behavior: PUT → 200, DELETE → 204. [`MockObjectServer::set_response`]
//! overrides every subsequent request's status/body (e.g. 403 with an XML error
//! document, 404 for idempotent-delete tests).

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One captured request, for shape assertions.
#[derive(Debug, Clone)]
pub(crate) struct CapturedRequest {
    pub request_line: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl CapturedRequest {
    /// Case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub(crate) struct MockObjectServer {
    pub base_url: String,
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    status: Arc<AtomicU16>,
    body: Arc<StdMutex<Vec<u8>>>,
}

impl MockObjectServer {
    /// All requests received so far, in arrival order. Each request is pushed
    /// before its response is written, so a call that returned already has its
    /// capture visible here.
    pub fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    /// Force `status`/`body` for every subsequent request (0 = default behavior).
    pub fn set_response(&self, status: u16, body: &[u8]) {
        self.status.store(status, Ordering::SeqCst);
        *self.body.lock().expect("body lock") = body.to_vec();
    }
}

pub(crate) async fn serve() -> MockObjectServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test server should bind");
    let address = listener
        .local_addr()
        .expect("test server should have local address");
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let captured = Arc::clone(&requests);
    let status = Arc::new(AtomicU16::new(0));
    let status_spawn = Arc::clone(&status);
    let body = Arc::new(StdMutex::new(Vec::new()));
    let body_spawn = Arc::clone(&body);

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let captured = Arc::clone(&captured);
            let status = Arc::clone(&status_spawn);
            let body = Arc::clone(&body_spawn);
            tokio::spawn(async move {
                let raw = read_request(&mut socket).await;
                captured
                    .lock()
                    .expect("requests lock")
                    .push(parse_request(&raw));
                let forced = status.load(Ordering::SeqCst);
                let (status_code, reason) = if forced == 0 {
                    if raw.starts_with("PUT ") {
                        (200, "OK")
                    } else {
                        (204, "No Content")
                    }
                } else {
                    (forced, "Reply")
                };
                let resp_body = body.lock().expect("body lock").clone();
                let response = format!(
                    "HTTP/1.1 {status_code} {reason}\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    resp_body.len()
                );
                let mut out = response.into_bytes();
                out.extend_from_slice(&resp_body);
                let _ = socket.write_all(&out).await;
            });
        }
    });

    MockObjectServer {
        base_url: format!("http://{address}"),
        requests,
        status,
        body,
    }
}

/// Read one full HTTP request (headers + body, per content-length).
async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = socket.read(&mut chunk).await.expect("read request");
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
        let Some(header_end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if buffer.len() >= header_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

fn parse_request(raw: &str) -> CapturedRequest {
    let mut lines = raw.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    let body = raw
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .as_bytes()
        .to_vec();
    CapturedRequest {
        request_line,
        headers,
        body,
    }
}
