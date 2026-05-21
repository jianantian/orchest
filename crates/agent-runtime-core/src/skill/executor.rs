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
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| ScriptError {
            message: format!("failed to spawn process: {}", e),
            code: Some("SPAWN_ERROR".into()),
        })?;

        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(input_json).await;
            let _ = stdin.shutdown().await;
        }

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
