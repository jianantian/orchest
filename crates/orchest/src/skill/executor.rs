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
pub struct ExecutionContext {
    pub work_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub capabilities: Option<SkillCapabilities>,
    pub timeout: Option<Duration>,
    pub on_update: Option<mpsc::Sender<Value>>,
}

#[derive(Debug, Clone)]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
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
            message: format!("failed to spawn process: {}", e),
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
                    message: format!("process IO error: {}", e),
                    code: Some("IO_ERROR".into()),
                })?
        } else {
            wait_fut.await.map_err(|e| ScriptError {
                message: format!("process IO error: {}", e),
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

    #[tokio::test]
    async fn bare_executor_kills_child_on_timeout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let pid_file = tmp.path().join("child_pid.txt");
        let script = tmp.path().join("slow.py");
        let pid_path_str = pid_file.to_str().unwrap().replace('\\', "\\\\");

        std::fs::write(
            // allow-blocking-io: test-only setup
            &script,
            format!(
                r#"
import os, sys, time
with open("{pid_path_str}", "w") as f:
    f.write(str(os.getpid()))
# Hang indefinitely
time.sleep(300)
"#,
            ),
        )
        .expect("write script");

        let executor = BareSubprocessExecutor::new();
        let tool_def = BundledToolDef {
            name: "slow_tool".into(),
            description: "".into(),
            executable: "python3".into(),
            script: script.clone(),
            input_schema: serde_json::json!({"type": "object"}),
        };

        let ctx = ExecutionContext {
            work_dir: tmp.path().to_path_buf(),
            env: std::env::vars().collect(),
            capabilities: None,
            timeout: Some(Duration::from_millis(500)),
            on_update: None,
        };

        let result = executor.execute(&tool_def, &script, &[], b"{}", &ctx).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code.as_deref(), Some("TIMEOUT"));

        // Read the PID and verify the child was killed
        let pid_str = std::fs::read_to_string(&pid_file).expect("read pid"); // allow-blocking-io: test-only
        let pid: u32 = pid_str.trim().parse().expect("parse pid");

        // Give the OS a moment to reap
        tokio::time::sleep(Duration::from_millis(200)).await;
        #[cfg(unix)]
        {
            // SAFETY: pid is a valid child process ID obtained from the child
            // that was spawned and later killed by `kill_on_drop`. Sending
            // signal 0 checks only whether the process still exists without
            // delivering a real signal. This is test-only, platform-gated
            // behind `#[cfg(unix)]`.
            let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
            assert!(!alive, "child process {pid} should be dead after timeout");
        }
    }
}
