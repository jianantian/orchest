//! Code execution tools: persistent Python sessions and one-shot JavaScript.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::events::RuntimeEvent;
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const MAX_TIMEOUT_SECONDS: u64 = 300;
const MAX_PRE_SENTINEL_LINES: usize = 10_000;
const SENTINEL_PREFIX: &str = "__ORCHEST_DONE__";
const DEFAULT_PYTHON_BIN: &str = "python3";

/// Return the Python binary path, allowing override via `PYTHON_BIN`.
fn python_bin() -> String {
    std::env::var("PYTHON_BIN").unwrap_or_else(|_| DEFAULT_PYTHON_BIN.into())
}

pub struct CodeExecutionMcpServer;

impl CodeExecutionMcpServer {
    pub fn tools() -> Vec<Arc<dyn Tool>> {
        let python_session = Arc::new(Mutex::new(None));
        vec![
            Arc::new(ExecutePythonTool::new(python_session)),
            Arc::new(ExecuteJavaScriptTool::new()),
        ]
    }
}

struct PythonSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub struct ExecutePythonTool {
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    session: Arc<Mutex<Option<PythonSession>>>,
}

impl ExecutePythonTool {
    fn new(session: Arc<Mutex<Option<PythonSession>>>) -> Self {
        Self {
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "default": 30, "maximum": 300 }
                },
                "required": ["code"]
            }),
            metadata: ToolMetadata {
                side_effect: true,
                approval: crate::tool::Approval::Never,
                timeout: Some(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS)),
                source: ToolSource::Builtin,
                ..ToolMetadata::default()
            },
            session,
        }
    }
}

#[async_trait]
impl Tool for ExecutePythonTool {
    fn name(&self) -> &str {
        "execute_python"
    }

    fn description(&self) -> &str {
        "Execute trusted Python code in a persistent per-run session. Input: code and optional timeout_seconds."
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

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let code = input.get("code").and_then(Value::as_str).ok_or_else(|| {
            ToolError::fatal("execute_python input missing code").with_code("INVALID_INPUT")
        })?;
        let timeout = requested_timeout(&input);
        let fut = execute_python_in_session(&self.session, code, ctx);
        match tokio::time::timeout(timeout, fut).await {
            Ok(result) => result.map(ToolOutput::Immediate),
            Err(_) => {
                let mut session = self.session.lock().await;
                if let Some(mut session) = session.take() {
                    let _ = session.child.kill().await;
                }
                Ok(ToolOutput::Immediate(json!({
                    "stdout": "",
                    "stderr": "timeout",
                    "exit_code": -1
                })))
            }
        }
    }
}

async fn execute_python_in_session(
    session: &Arc<Mutex<Option<PythonSession>>>,
    code: &str,
    ctx: &ToolContext,
) -> Result<Value, ToolError> {
    let mut guard = session.lock().await;
    if guard.is_none() {
        *guard = Some(spawn_python_session().await?);
    }
    let session = guard.as_mut().ok_or_else(|| {
        ToolError::fatal("python session was not initialized").with_code("SESSION_ERROR")
    })?;

    let wrapper_body = format!(
        r#"
import contextlib, io, json, traceback
__orchest_stdout = io.StringIO()
__orchest_stderr = io.StringIO()
try:
    with contextlib.redirect_stdout(__orchest_stdout), contextlib.redirect_stderr(__orchest_stderr):
        exec({code_json}, globals())
except Exception:
    traceback.print_exc(file=__orchest_stderr)
print("{sentinel}" + json.dumps({{"stdout": __orchest_stdout.getvalue(), "stderr": __orchest_stderr.getvalue(), "exit_code": 0 if __orchest_stderr.getvalue() == "" else 1}}), flush=True)
"#,
        code_json = serde_json::to_string(code).map_err(|e| ToolError::fatal(format!(
            "failed to encode python code: {e}"
        ))
        .with_code("SERIALIZATION_ERROR"))?,
        sentinel = SENTINEL_PREFIX
    );
    let wrapper = format!(
        "exec({})\n",
        serde_json::to_string(&wrapper_body).map_err(|e| ToolError::fatal(format!(
            "failed to encode python wrapper: {e}"
        ))
        .with_code("SERIALIZATION_ERROR"))?
    );

    session
        .stdin
        .write_all(wrapper.as_bytes())
        .await
        .map_err(io_tool_error)?;
    session.stdin.flush().await.map_err(io_tool_error)?;

    let mut line_count = 0usize;
    loop {
        let mut line = String::new();
        let read = session
            .stdout
            .read_line(&mut line)
            .await
            .map_err(io_tool_error)?;
        if read == 0 {
            if let Some(mut dead) = guard.take() {
                let _ = dead.child.kill().await;
            }
            return Err(ToolError::fatal("python session exited").with_code("SESSION_EXITED"));
        }
        let trimmed = line.trim_end();
        if let Some(payload) = trimmed.strip_prefix(SENTINEL_PREFIX) {
            let output: Value = serde_json::from_str(payload).map_err(|e| {
                ToolError::fatal(format!("invalid python sentinel payload: {e}"))
                    .with_code("INVALID_OUTPUT")
            })?;
            if let Some(stdout) = output.get("stdout").and_then(Value::as_str) {
                for stdout_line in stdout.lines() {
                    emit_update(ctx, json!({"stdout_line": stdout_line})).await;
                }
            }
            return Ok(output);
        }
        line_count += 1;
        emit_update(ctx, json!({"stdout_line": trimmed})).await;
        if line_count >= MAX_PRE_SENTINEL_LINES {
            let _ = session.child.kill().await;
            *guard = None;
            return Err(
                ToolError::fatal("python output exceeded line limit before sentinel")
                    .with_code("OUTPUT_LIMIT"),
            );
        }
    }
}

async fn spawn_python_session() -> Result<PythonSession, ToolError> {
    let bin = python_bin();
    let mut child = Command::new(&bin)
        .arg("-u")
        .arg("-i")
        .arg("-q")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            ToolError::fatal(format!("failed to spawn {bin}: {e}")).with_code("SPAWN_ERROR")
        })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| ToolError::fatal("python stdin unavailable").with_code("SESSION_ERROR"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::fatal("python stdout unavailable").with_code("SESSION_ERROR"))?;
    Ok(PythonSession {
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

pub struct ExecuteJavaScriptTool {
    input_schema: JsonSchema,
    metadata: ToolMetadata,
}

impl ExecuteJavaScriptTool {
    fn new() -> Self {
        Self {
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "default": 30, "maximum": 300 }
                },
                "required": ["code"]
            }),
            metadata: ToolMetadata {
                side_effect: true,
                approval: crate::tool::Approval::Never,
                timeout: Some(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS)),
                source: ToolSource::Builtin,
                ..ToolMetadata::default()
            },
        }
    }
}

#[async_trait]
impl Tool for ExecuteJavaScriptTool {
    fn name(&self) -> &str {
        "execute_javascript"
    }

    fn description(&self) -> &str {
        "Execute trusted JavaScript code. Uses Deno when available; falls back to stateless node -e without cross-call variables."
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
        let code = input.get("code").and_then(Value::as_str).ok_or_else(|| {
            ToolError::fatal("execute_javascript input missing code").with_code("INVALID_INPUT")
        })?;
        let timeout = requested_timeout(&input);
        let use_deno = which::which("deno").is_ok();
        let mut command = if use_deno {
            let mut cmd = Command::new("deno");
            cmd.arg("run")
                .arg("--allow-net")
                .arg("--allow-read")
                .arg("-");
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg("-e").arg(code);
            cmd
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|e| {
            ToolError::fatal(format!("failed to spawn JavaScript runtime: {e}"))
                .with_code("SPAWN_ERROR")
        })?;
        if use_deno {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin
                    .write_all(code.as_bytes())
                    .await
                    .map_err(io_tool_error)?;
                stdin.shutdown().await.map_err(io_tool_error)?;
            }
        }
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => return Err(io_tool_error(error)),
            Err(_) => {
                return Ok(ToolOutput::Immediate(json!({
                    "stdout": "",
                    "stderr": "timeout",
                    "exit_code": -1
                })));
            }
        };
        Ok(ToolOutput::Immediate(json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code().unwrap_or(-1)
        })))
    }
}

fn requested_timeout(input: &Value) -> Duration {
    let seconds = input
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .min(MAX_TIMEOUT_SECONDS);
    Duration::from_secs(seconds)
}

async fn emit_update(ctx: &ToolContext, partial: Value) {
    if let Some(tx) = &ctx.event_tx {
        let _ = tx
            .send(RuntimeEvent::ToolCallUpdate {
                tool: "execute_python".into(),
                tool_call_id: ctx.tool_call_id.clone(),
                partial,
            })
            .await;
    }
}

fn io_tool_error(error: std::io::Error) -> ToolError {
    ToolError::transient(error.to_string()).with_code("IO_ERROR")
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;

    fn tool_context(event_tx: Option<mpsc::Sender<RuntimeEvent>>) -> ToolContext {
        ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "call-1".into(),
            event_tx,
            webhook_base_url: None,
            approval_bus: crate::run::handle::ApprovalBus::default(),
            remaining_budget: crate::budget::BudgetConfig::default(),
            parent_messages: vec![],
        }
    }

    #[tokio::test]
    async fn python_session_returns_output_limit_before_sentinel() {
        let session = Arc::new(Mutex::new(None));
        let ctx = tool_context(None);
        let code = r#"
import sys, time
sys.__stdout__.write("line\n" * 10001)
sys.__stdout__.flush()
time.sleep(60)
"#;

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            execute_python_in_session(&session, code, &ctx),
        )
        .await
        .expect("line limit should return before timeout");

        let err = result.expect_err("pre-sentinel output should exceed limit");
        assert_eq!(err.code.as_deref(), Some("OUTPUT_LIMIT"));
    }

    #[tokio::test]
    async fn python_session_emits_update_for_pre_sentinel_stdout() {
        let session = Arc::new(Mutex::new(None));
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let ctx = tool_context(Some(event_tx));
        let code = r#"
import sys
sys.__stdout__.write("prelude\n")
sys.__stdout__.flush()
"#;

        let output = tokio::time::timeout(
            Duration::from_secs(2),
            execute_python_in_session(&session, code, &ctx),
        )
        .await
        .expect("sentinel output should arrive before timeout")
        .expect("sentinel output should parse");

        assert_eq!(output["exit_code"], 0);
        let event = event_rx.recv().await.expect("pre-sentinel update event");
        assert!(matches!(
            event,
            RuntimeEvent::ToolCallUpdate { partial, .. }
                if partial["stdout_line"] == "prelude"
        ));
    }
}
