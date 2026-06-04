//! MCP (Model Context Protocol) client and tool implementations.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

use super::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub server_id: String,
    pub transport: McpTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    Stdio { command: String, args: Vec<String> },
    StreamableHttp { url: String, auth: Option<McpAuth> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpAuth {
    Bearer { token: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct McpError {
    pub message: String,
    pub code: Option<String>,
}

impl From<McpError> for ToolError {
    fn from(value: McpError) -> Self {
        Self {
            message: value.message,
            code: value.code,
        }
    }
}

pub struct McpStdioClient {
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: Arc<AtomicU64>,
    child: Arc<Mutex<Child>>,
    reader_abort: tokio::task::AbortHandle,
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        self.reader_abort.abort();
        // Kill the child process so we don't leave zombie MCP servers.
        // We use try_lock since we're in synchronous Drop.
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.start_kill();
        }
    }
}

impl McpStdioClient {
    pub async fn connect(command: &str, args: &[&str]) -> Result<Self, McpError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| McpError {
                message: format!("failed to spawn MCP server '{command}': {e}"),
                code: Some("spawn_failed".into()),
            })?;

        let stdin = child.stdin.take().ok_or_else(|| McpError {
            message: "MCP server stdin was not captured".into(),
            code: Some("missing_stdin".into()),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| McpError {
            message: "MCP server stdout was not captured".into(),
            code: Some("missing_stdout".into()),
        })?;
        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
        let pending_for_reader = Arc::clone(&pending);
        let reader_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let response = match serde_json::from_str::<Value>(&line) {
                    Ok(response) => response,
                    Err(_) => {
                        pending_for_reader.lock().await.clear();
                        break;
                    }
                };
                let Some(id) = response.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                if let Some(tx) = pending_for_reader.lock().await.remove(&id) {
                    let _ = tx.send(response);
                }
            }
            pending_for_reader.lock().await.clear();
        });

        let client = Self {
            stdin,
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
            child: Arc::new(Mutex::new(child)),
            reader_abort: reader_task.abort_handle(),
        };
        client.initialize().await?;
        Ok(client)
    }

    pub async fn connect_owned(command: &str, args: &[String]) -> Result<Self, McpError> {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        Self::connect(command, &refs).await
    }

    async fn initialize(&self) -> Result<Value, McpError> {
        self.send_request(
            "initialize",
            json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "orchest", "version": "0.2.0"}}),
        )
        .await
    }

    pub async fn list_tools(&self) -> Result<Vec<McpToolDef>, McpError> {
        let result = self.send_request("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| McpError {
                message: "MCP tools/list response missing tools array".into(),
                code: Some("invalid_response".into()),
            })?;

        let mut defs = Vec::new();
        for tool in tools {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| McpError {
                    message: "MCP tool missing name".into(),
                    code: Some("invalid_tool".into()),
                })?;
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let input_schema = tool
                .get("inputSchema")
                .or_else(|| tool.get("input_schema"))
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"}));
            defs.push(McpToolDef {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
            });
        }
        Ok(defs)
    }

    pub async fn call_tool(&self, name: &str, input: Value) -> Result<Value, McpError> {
        self.send_request("tools/call", json!({"name": name, "arguments": input}))
            .await
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut line = match serde_json::to_vec(&request) {
            Ok(line) => line,
            Err(error) => {
                self.pending.lock().await.remove(&id);
                return Err(McpError {
                    message: format!("failed to serialize MCP request: {error}"),
                    code: Some("serialize_failed".into()),
                });
            }
        };
        line.push(b'\n');

        {
            let mut stdin = self.stdin.lock().await;
            if let Err(error) = stdin.write_all(&line).await {
                self.pending.lock().await.remove(&id);
                return Err(McpError {
                    message: format!("failed to write MCP request: {error}"),
                    code: Some("write_failed".into()),
                });
            }
            if let Err(error) = stdin.flush().await {
                self.pending.lock().await.remove(&id);
                return Err(McpError {
                    message: format!("failed to flush MCP request: {error}"),
                    code: Some("write_failed".into()),
                });
            }
        }

        let response = rx.await.map_err(|_| McpError {
            message: "MCP server disconnected before responding".into(),
            code: Some("process_exited".into()),
        })?;
        parse_rpc_response(response)
    }
}

pub struct McpHttpClient {
    url: String,
    auth: Option<McpAuth>,
    client: reqwest::Client,
    timeout: Duration,
    next_id: AtomicU64,
}

impl McpHttpClient {
    pub async fn connect(url: &str, auth: Option<McpAuth>) -> Result<Self, McpError> {
        if url.trim().is_empty() {
            return Err(McpError {
                message: "MCP HTTP URL cannot be empty".into(),
                code: Some("invalid_url".into()),
            });
        }
        let client = Self {
            url: url.trim_end_matches('/').to_string(),
            auth,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(30),
            next_id: AtomicU64::new(1),
        };
        client
            .request("initialize", json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "orchest", "version": "0.2.0"}}), None)
            .await?;
        Ok(client)
    }

    pub async fn list_tools(&self) -> Result<Vec<McpToolDef>, McpError> {
        let result = self.request("tools/list", json!({}), None).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| McpError {
                message: "MCP tools/list response missing tools array".into(),
                code: Some("invalid_response".into()),
            })?;
        Ok(tools
            .iter()
            .filter_map(|tool| {
                Some(McpToolDef {
                    name: tool.get("name")?.as_str()?.to_string(),
                    description: tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    input_schema: tool
                        .get("inputSchema")
                        .or_else(|| tool.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"})),
                })
            })
            .collect())
    }

    pub async fn call_tool(&self, name: &str, input: Value) -> Result<Value, McpError> {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": input}),
            None,
        )
        .await
    }

    pub async fn call_tool_with_timeout(
        &self,
        name: &str,
        input: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, McpError> {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": input}),
            timeout,
        )
        .await
    }

    /// Whether a method is safe to retry.  Only discovery and control
    /// operations (`initialize`, `tools/list`) may be retried because
    /// they are idempotent.  `tools/call` is never retried by default
    /// because it may trigger side effects and duplicating a call after
    /// a transient failure is unsafe without an idempotency signal.
    fn is_retryable(method: &str) -> bool {
        matches!(method, "initialize" | "tools/list")
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, McpError> {
        let max_attempts = if Self::is_retryable(method) { 4 } else { 1 };
        let mut delay = Duration::from_millis(50);
        let mut last_error = None;
        for _ in 0..max_attempts {
            match self.request_once(method, params.clone(), timeout).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    last_error = Some(error);
                    if max_attempts > 1 {
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| McpError {
            message: "MCP HTTP request failed".into(),
            code: Some("request_failed".into()),
        }))
    }

    async fn request_once(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut request = self
            .client
            .post(&self.url)
            .timeout(timeout.unwrap_or(self.timeout))
            .header("accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(McpAuth::Bearer { token }) = &self.auth {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| McpError {
            message: e.to_string(),
            code: Some("request_failed".into()),
        })?;
        if !response.status().is_success() {
            let status = response.status();
            return Err(McpError {
                message: format!("MCP HTTP returned {status}"),
                code: Some(status.as_u16().to_string()),
            });
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if content_type.contains("text/event-stream") {
            parse_sse_response(response).await
        } else {
            parse_rpc_response(response.json().await.map_err(|e| McpError {
                message: e.to_string(),
                code: Some("invalid_json".into()),
            })?)
        }
    }
}

async fn parse_sse_response(response: reqwest::Response) -> Result<Value, McpError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| McpError {
            message: e.to_string(),
            code: Some("stream_failed".into()),
        })?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buffer.find("\n\n") {
            let block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();
            let data = block
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap_or_default();
            if data.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(data).map_err(|e| McpError {
                message: e.to_string(),
                code: Some("invalid_json".into()),
            })?;
            return parse_rpc_response(value);
        }
    }
    Err(McpError {
        message: "MCP SSE stream ended without response".into(),
        code: Some("empty_stream".into()),
    })
}

fn parse_rpc_response(value: Value) -> Result<Value, McpError> {
    if let Some(error) = value.get("error") {
        return Err(McpError {
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("MCP request failed")
                .to_string(),
            code: error.get("code").map(ToString::to_string),
        });
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

#[derive(Clone)]
pub enum McpClient {
    Stdio(Arc<McpStdioClient>),
    Http(Arc<McpHttpClient>),
}

pub struct McpTool {
    name: String,
    description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    client: McpClient,
}

impl McpTool {
    pub fn new(server_id: String, def: McpToolDef, client: McpClient) -> Self {
        Self {
            name: def.name,
            description: def.description,
            input_schema: def.input_schema,
            metadata: ToolMetadata {
                side_effect: true,
                approval: crate::tool::Approval::Never,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::McpServer { server_id },
            },
            client,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let value = match &self.client {
            McpClient::Stdio(client) => client.call_tool(&self.name, input).await?,
            McpClient::Http(client) => {
                client
                    .call_tool_with_timeout(&self.name, input, self.metadata.timeout)
                    .await?
            }
        };
        Ok(ToolOutput::Immediate(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn pending_map_routes_responses_by_id() {
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
        let (tx1, rx1) = oneshot::channel::<Value>();
        let (tx2, rx2) = oneshot::channel::<Value>();
        pending.lock().await.insert(1, tx1);
        pending.lock().await.insert(2, tx2);

        let resp1 = json!({"jsonrpc": "2.0", "id": 1, "result": "a"});
        let resp2 = json!({"jsonrpc": "2.0", "id": 2, "result": "b"});
        if let Some(tx) = pending.lock().await.remove(&resp2["id"].as_u64().unwrap()) {
            let _ = tx.send(resp2);
        }
        if let Some(tx) = pending.lock().await.remove(&resp1["id"].as_u64().unwrap()) {
            let _ = tx.send(resp1);
        }

        assert_eq!(rx1.await.unwrap()["result"], "a");
        assert_eq!(rx2.await.unwrap()["result"], "b");
    }

    #[tokio::test]
    async fn stdio_client_lists_and_calls_tools() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let server = tmp.path().join("mcp_server.py");
        fs::write(
            &server,
            r#"
import json, sys
for line in sys.stdin:
    req = json.loads(line)
    method = req.get("method")
    if method == "initialize":
        result = {"capabilities": {"tools": {}}}
    elif method == "tools/list":
        result = {"tools": [{"name": "read_file", "description": "Read a file", "inputSchema": {"type": "object"}}]}
    elif method == "tools/call":
        with open(req["params"]["arguments"]["path"]) as f:
            text = f.read()
        result = {"content": [{"type": "text", "text": text}]}
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req["id"], "result": result}) + "\n")
    sys.stdout.flush()
"#,
        )
        .expect("server write");
        let data = tmp.path().join("data.txt");
        fs::write(&data, "hello mcp").expect("data write");

        let client = McpStdioClient::connect(
            "python3",
            &[server.to_str().expect("server path should be utf-8")],
        )
        .await
        .expect("stdio client connects");
        let tools = client.list_tools().await.expect("list tools");
        assert_eq!(tools[0].name, "read_file");

        let output = client
            .call_tool("read_file", json!({"path": data}))
            .await
            .expect("call tool");
        assert_eq!(output["content"][0]["text"], "hello mcp");
    }

    #[tokio::test]
    async fn http_client_reads_sse_tool_response() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let request_count = Arc::new(AtomicU32::new(0));
        tokio::spawn(async move {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let request_count = Arc::clone(&request_count);
                tokio::spawn(async move {
                    let mut buffer = vec![0; 4096];
                    let _read = socket.read(&mut buffer).await.expect("read");
                    let index = request_count.fetch_add(1, Ordering::SeqCst);
                    let body = if index == 1 {
                        r#"data: {"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"ping","description":"Ping tool","inputSchema":{"type":"object"}}]}}"#
                    } else if index == 2 {
                        r#"data: {"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"pong"}]}}"#
                    } else {
                        r#"data: {"jsonrpc":"2.0","id":1,"result":{"capabilities":{"tools":{}}}}"#
                    };
                    let body = format!("{body}\n\n");
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket.write_all(response.as_bytes()).await.expect("write");
                });
            }
        });

        let client = McpHttpClient::connect(&format!("http://{address}"), None)
            .await
            .expect("connect");
        let tools = client.list_tools().await.expect("list tools");
        assert_eq!(tools[0].name, "ping");
        let output = client
            .call_tool("ping", json!({}))
            .await
            .expect("call tool");
        assert_eq!(output["content"][0]["text"], "pong");
    }

    /// Verify that `tools/call` is sent exactly once even when the
    /// first attempt fails (no retry for side-effectful methods).
    #[tokio::test]
    async fn http_tools_call_does_not_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_spawn = Arc::clone(&call_count);

        tokio::spawn(async move {
            // Accept initialize (index 0), then tools/call (index 1).
            // tools/call deliberately returns an error to see if the
            // client retries.
            for _ in 0..4 {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let call_count = Arc::clone(&call_count_spawn);
                tokio::spawn(async move {
                    let request_text = read_http_request(&mut socket).await;
                    let is_tools_call = request_text.contains("tools/call");
                    if is_tools_call {
                        call_count.fetch_add(1, Ordering::SeqCst);
                    }
                    // Return an error so retryable methods would retry.
                    let body = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"transient failure"}}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket.write_all(response.as_bytes()).await.expect("write");
                });
            }
        });

        // Build client manually to skip initialize (which would also
        // hit the error server).
        let client = McpHttpClient {
            url: format!("http://{address}"),
            auth: None,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(5),
            next_id: AtomicU64::new(1),
        };

        let result = client.call_tool("do_something", json!({})).await;
        assert!(result.is_err(), "tools/call should fail");

        // Give a moment for any (incorrect) retries to land
        tokio::time::sleep(Duration::from_millis(200)).await;
        let count = call_count.load(Ordering::SeqCst);
        assert_eq!(
            count, 1,
            "tools/call should be sent exactly once, not retried (got {count})"
        );
    }

    async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = vec![0; 1024];
        loop {
            let n = socket.read(&mut chunk).await.expect("read");
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
            let Some(header_end) = find_header_end(&buffer) else {
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

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    #[tokio::test]
    async fn stdio_client_kills_child_on_drop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let pid_file = tmp.path().join("mcp_pid.txt");
        let server = tmp.path().join("mcp_long_server.py");
        let pid_path_str = pid_file.to_str().unwrap().replace('\\', "\\\\");
        fs::write(
            &server,
            format!(
                r#"
import json, sys, os, time
# Write our PID so the test can check if we're still alive
with open("{pid_path_str}", "w") as f:
    f.write(str(os.getpid()))
for line in sys.stdin:
    req = json.loads(line)
    result = {{"capabilities": {{"tools": {{}}}}}}
    sys.stdout.write(json.dumps({{"jsonrpc": "2.0", "id": req["id"], "result": result}}) + "\n")
    sys.stdout.flush()
# If stdin closes, sleep long enough for the test to check
time.sleep(60)
"#,
            ),
        )
        .expect("server write");

        let client = McpStdioClient::connect("python3", &[server.to_str().unwrap()])
            .await
            .expect("stdio client connects");

        // Read the PID
        let pid_str = fs::read_to_string(&pid_file).expect("read pid file");
        let pid: u32 = pid_str.trim().parse().expect("parse pid");

        // Verify the process is alive
        assert!(
            is_process_alive(pid),
            "MCP child should be alive before drop"
        );

        // Drop the client — this should kill the child
        drop(client);

        // Give the OS a moment to deliver the kill signal
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(
            !is_process_alive(pid),
            "MCP child should be dead after client drop"
        );
    }

    /// HTTP 4xx/5xx responses surface the status code in the error.
    #[tokio::test]
    async fn http_error_status_codes_surface_in_mcp_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let body = "Forbidden";
            let response = format!(
                "HTTP/1.1 403 Forbidden\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.expect("write");
        });

        // Build client manually to skip initialize
        let client = McpHttpClient {
            url: format!("http://{address}"),
            auth: None,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(5),
            next_id: AtomicU64::new(1),
        };

        let err = client
            .call_tool("test_tool", json!({}))
            .await
            .expect_err("should fail with 403");
        assert!(
            err.message.contains("403"),
            "error should mention HTTP status: {}",
            err.message
        );
        assert_eq!(err.code.as_deref(), Some("403"));
    }

    /// Connection failure includes useful error context.
    #[tokio::test]
    async fn http_connection_failure_includes_error_message() {
        let client = McpHttpClient {
            url: "http://127.0.0.1:1".to_string(), // port 1 should refuse
            auth: None,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(1),
            next_id: AtomicU64::new(1),
        };

        let err = client
            .call_tool("anything", json!({}))
            .await
            .expect_err("should fail to connect");
        assert!(
            !err.message.is_empty(),
            "error should have a descriptive message"
        );
        assert_eq!(err.code.as_deref(), Some("request_failed"));
    }

    #[cfg(unix)]
    // SAFETY: pid is the MCP child process ID obtained before dropping
    // the client.  Signal 0 is a no-op existence check — it never
    // terminates the process.  Test-only, gated behind `#[cfg(unix)]`.
    fn is_process_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(not(unix))]
    fn is_process_alive(_pid: u32) -> bool {
        // On non-unix, just return true to skip assertion
        true
    }
}
