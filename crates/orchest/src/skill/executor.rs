//! Script executors for running skill bundled-tool scripts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::skill::{BundledToolDef, SkillCapabilities};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExecutionContext {
    pub work_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub capabilities: Option<SkillCapabilities>,
    pub timeout: Option<Duration>,
    pub on_update: Option<mpsc::Sender<Value>>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
#[non_exhaustive]
pub struct ScriptError {
    pub message: String,
    pub code: Option<String>,
}

#[async_trait]
pub trait ScriptExecutor: Send + Sync {
    #[allow(clippy::too_many_arguments)] // justified: executor trait needs tool def, path, args, stdin, and context
    async fn execute(
        &self,
        tool: &BundledToolDef,
        script_path: &Path,
        args: &[String],
        input_json: &[u8],
        ctx: &ExecutionContext,
    ) -> Result<ScriptOutput, ScriptError>;
}

#[derive(Debug, Default)]
pub struct BareSubprocessExecutor;

impl BareSubprocessExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ScriptExecutor for BareSubprocessExecutor {
    async fn execute(
        &self,
        tool: &BundledToolDef,
        script_path: &Path,
        args: &[String],
        input_json: &[u8],
        ctx: &ExecutionContext,
    ) -> Result<ScriptOutput, ScriptError> {
        let exe_path = if tool.executable.contains(std::path::MAIN_SEPARATOR) {
            PathBuf::from(&tool.executable)
        } else {
            which::which(&tool.executable).map_err(|e| ScriptError {
                message: format!("executable '{}' not found in PATH: {}", tool.executable, e),
                code: Some("EXECUTABLE_NOT_FOUND".into()),
            })?
        };

        let mut cmd = Command::new(&exe_path);
        cmd.arg(script_path)
            .args(args)
            .current_dir(&ctx.work_dir)
            .env_clear()
            .envs(&ctx.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| ScriptError {
            message: format!("failed to spawn process: {e}"),
            code: Some("SPAWN_ERROR".into()),
        })?;

        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(input_json).await;
            let _ = stdin.shutdown().await;
        }
        // Drop stdin so the child can see EOF.
        child.stdin.take();

        let wait_fut = child.wait_with_output();
        let output = if let Some(timeout) = ctx.timeout {
            tokio::time::timeout(timeout, wait_fut)
                .await
                .map_err(|_| ScriptError {
                    message: "script execution timed out".into(),
                    code: Some("TIMEOUT".into()),
                })?
                .map_err(|e| ScriptError {
                    message: format!("process IO error: {e}"),
                    code: Some("IO_ERROR".into()),
                })?
        } else {
            wait_fut.await.map_err(|e| ScriptError {
                message: format!("process IO error: {e}"),
                code: Some("IO_ERROR".into()),
            })?
        };

        Ok(ScriptOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unix-only: the child is a `/bin/sh` script and liveness is checked with
    // `kill(pid, 0)`.
    #[cfg(unix)]
    #[tokio::test]
    async fn bare_executor_kills_child_on_timeout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let pid_file = tmp.path().join("child_pid.txt");
        let script = tmp.path().join("slow.sh");

        // `sh` starts in milliseconds, so the PID file is written well inside
        // the timeout even on a loaded CI runner (a `python3` child could be
        // killed before its interpreter finished starting). `exec` keeps the
        // PID, so the recorded PID is the process the executor must kill.
        std::fs::write(
            // allow-blocking-io: test-only setup
            &script,
            "echo $$ > child_pid.txt\nexec sleep 300\n",
        )
        .expect("write script");

        let executor = BareSubprocessExecutor::new();
        let tool_def = BundledToolDef {
            name: "slow_tool".into(),
            description: "".into(),
            executable: "/bin/sh".into(),
            script: script.clone(),
            input_schema: serde_json::json!({"type": "object"}),
        };

        let ctx = ExecutionContext {
            work_dir: tmp.path().to_path_buf(),
            env: std::env::vars().collect(),
            capabilities: None,
            timeout: Some(Duration::from_secs(2)),
            on_update: None,
        };

        let result = executor.execute(&tool_def, &script, &[], b"{}", &ctx).await;

        let err = result.expect_err("a hanging script must time out");
        assert_eq!(err.code.as_deref(), Some("TIMEOUT"));

        let pid_str = std::fs::read_to_string(&pid_file) // allow-blocking-io: test-only
            .expect("child must record its PID before the 2 s timeout");
        let pid: i32 = pid_str.trim().parse().expect("parse pid");

        // `kill_on_drop` sends SIGKILL; tokio reaps the orphan asynchronously,
        // and an unreaped zombie still answers `kill(pid, 0)`. Poll instead of
        // sleeping a fixed amount.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: pid came from the child the executor spawned. Signal 0
            // only checks whether the process exists; nothing is delivered.
            let alive = unsafe { libc::kill(pid, 0) == 0 };
            if !alive {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "child process {pid} should be dead after timeout"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
