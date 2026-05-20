use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::skill::BundledToolDef;
use crate::tool::async_job::{JobHandle, JobStatus};
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

const ALLOWED_EXECUTABLES: &[&str] = &["python", "python3", "node", "bash", "sh"];

#[derive(Debug)]
pub struct SkillBundledTool {
    name: String,
    description: String,
    executable: String,
    script: PathBuf,
    skill_dir: PathBuf,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
}

impl SkillBundledTool {
    pub fn new(
        def: &BundledToolDef,
        skill_dir: PathBuf,
        skill_name: String,
    ) -> Result<Self, ToolError> {
        if !ALLOWED_EXECUTABLES.contains(&def.executable.as_str()) {
            return Err(ToolError {
                message: format!(
                    "executable '{}' not in whitelist: {:?}",
                    def.executable, ALLOWED_EXECUTABLES
                ),
                code: Some("INVALID_EXECUTABLE".into()),
            });
        }

        let script_abs = skill_dir.join(&def.script);
        let script_resolved = script_abs.canonicalize().map_err(|e| ToolError {
            message: format!(
                "failed to resolve script path '{}': {}",
                def.script.display(),
                e
            ),
            code: Some("SCRIPT_NOT_FOUND".into()),
        })?;

        let skill_dir_resolved = skill_dir.canonicalize().map_err(|e| ToolError {
            message: format!(
                "failed to resolve skill dir '{}': {}",
                skill_dir.display(),
                e
            ),
            code: Some("SKILL_DIR_ERROR".into()),
        })?;

        if !script_resolved.starts_with(&skill_dir_resolved) {
            return Err(ToolError {
                message: format!(
                    "script path '{}' escapes skill directory '{}'",
                    def.script.display(),
                    skill_dir.display()
                ),
                code: Some("PATH_TRAVERSAL".into()),
            });
        }

        Ok(Self {
            name: def.name.clone(),
            description: def.description.clone(),
            executable: def.executable.clone(),
            script: script_resolved,
            skill_dir: skill_dir_resolved,
            input_schema: def.input_schema.clone(),
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::Skill { skill_name },
            },
        })
    }

    fn resolve_executable(&self) -> Result<PathBuf, ToolError> {
        which::which(&self.executable).map_err(|e| ToolError {
            message: format!("executable '{}' not found in PATH: {}", self.executable, e),
            code: Some("EXECUTABLE_NOT_FOUND".into()),
        })
    }

    async fn spawn_script(
        &self,
        input_json: &[u8],
        extra_args: &[&str],
        timeout: Option<Duration>,
    ) -> Result<(String, String, i32), ToolError> {
        let exe_path = self.resolve_executable()?;

        let mut cmd = Command::new(&exe_path);
        cmd.arg(&self.script);
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.current_dir(&self.skill_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| ToolError {
            message: format!("failed to spawn process: {}", e),
            code: Some("SPAWN_ERROR".into()),
        })?;

        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(input_json).await;
            let _ = stdin.shutdown().await;
        }

        let wait_fut = child.wait_with_output();
        let output = if let Some(t) = timeout {
            tokio::time::timeout(t, wait_fut)
                .await
                .map_err(|_| ToolError {
                    message: "script execution timed out".into(),
                    code: Some("TIMEOUT".into()),
                })?
                .map_err(|e| ToolError {
                    message: format!("process IO error: {}", e),
                    code: Some("IO_ERROR".into()),
                })?
        } else {
            wait_fut.await.map_err(|e| ToolError {
                message: format!("process IO error: {}", e),
                code: Some("IO_ERROR".into()),
            })?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, code))
    }
}

#[async_trait]
impl Tool for SkillBundledTool {
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
        let input_json = serde_json::to_vec(&input).map_err(|e| ToolError {
            message: format!("failed to serialize input: {}", e),
            code: Some("SERIALIZATION_ERROR".into()),
        })?;

        let (stdout, stderr, exit_code) = self
            .spawn_script(&input_json, &[], self.metadata.timeout)
            .await?;

        if !stderr.is_empty() {
            eprintln!("[skill:{}] stderr: {}", self.name, stderr.trim());
        }

        if exit_code != 0 {
            return Err(ToolError {
                message: format!("script exited with code {}: {}", exit_code, stderr.trim()),
                code: Some("NON_ZERO_EXIT".into()),
            });
        }

        let parsed: Value = serde_json::from_str(stdout.trim()).map_err(|e| ToolError {
            message: format!("failed to parse script stdout as JSON: {}", e),
            code: Some("INVALID_OUTPUT".into()),
        })?;

        if parsed.get("__async_job").and_then(|v| v.as_bool()) == Some(true) {
            let job_id = parsed
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError {
                    message: "async job response missing 'job_id'".into(),
                    code: Some("MISSING_JOB_ID".into()),
                })?
                .to_string();

            let poll_interval_secs = parsed
                .get("poll_interval")
                .and_then(|v| v.as_u64())
                .unwrap_or(5);

            let exe = self.executable.clone();
            let script = self.script.clone();
            let skill_dir = self.skill_dir.clone();
            let tool_name = self.name.clone();
            let poll_job_id = job_id.clone();

            let poll_fn = move || {
                let exe = exe.clone();
                let script = script.clone();
                let skill_dir = skill_dir.clone();
                let tool_name = tool_name.clone();
                let poll_job_id = poll_job_id.clone();

                Box::pin(async move {
                    let exe_path = which::which(&exe).map_err(|e| ToolError {
                        message: format!("executable '{}' not found: {}", exe, e),
                        code: Some("EXECUTABLE_NOT_FOUND".into()),
                    })?;

                    let mut cmd = Command::new(&exe_path);
                    cmd.arg(&script)
                        .arg("--poll")
                        .arg(&poll_job_id)
                        .current_dir(&skill_dir)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped());

                    let output = cmd.output().await.map_err(|e| ToolError {
                        message: format!("poll spawn error: {}", e),
                        code: Some("SPAWN_ERROR".into()),
                    })?;

                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.is_empty() {
                        eprintln!("[skill:{}:poll] stderr: {}", tool_name, stderr.trim());
                    }

                    if !output.status.success() {
                        return Err(ToolError {
                            message: format!(
                                "poll script exited with code {}: {}",
                                output.status.code().unwrap_or(-1),
                                stderr.trim()
                            ),
                            code: Some("NON_ZERO_EXIT".into()),
                        });
                    }

                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let parsed: Value =
                        serde_json::from_str(stdout.trim()).map_err(|e| ToolError {
                            message: format!("failed to parse poll output: {}", e),
                            code: Some("INVALID_OUTPUT".into()),
                        })?;

                    let status = parsed
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pending");

                    match status {
                        "completed" => {
                            let result = parsed.get("result").cloned().unwrap_or(Value::Null);
                            Ok(JobStatus::Completed(result))
                        }
                        "failed" => {
                            let error = parsed
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown error")
                                .to_string();
                            Ok(JobStatus::Failed(error))
                        }
                        _ => {
                            let progress = parsed
                                .get("progress")
                                .and_then(|v| v.as_f64())
                                .map(|v| v as f32);
                            Ok(JobStatus::Pending {
                                progress,
                                message: None,
                            })
                        }
                    }
                })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<JobStatus, ToolError>> + Send>,
                    >
            };

            return Ok(ToolOutput::AsyncJob(JobHandle {
                job_id,
                poll: Arc::new(poll_fn),
                poll_interval: Duration::from_secs(poll_interval_secs),
                timeout: self.metadata.timeout,
            }));
        }

        Ok(ToolOutput::Immediate(parsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_skill_dir(tmp: &std::path::Path, script_name: &str, script_content: &str) -> PathBuf {
        let skill_dir = tmp.join("test_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join(format!("scripts/{}", script_name)),
            script_content,
        )
        .unwrap();
        skill_dir
    }

    fn make_def(executable: &str, script: &str) -> BundledToolDef {
        BundledToolDef {
            name: "test_tool".into(),
            description: "A test tool".into(),
            executable: executable.into(),
            script: PathBuf::from(script),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn rejects_unlisted_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = create_skill_dir(tmp.path(), "run.rb", "");
        let def = make_def("ruby", "scripts/run.rb");

        let result = SkillBundledTool::new(&def, skill_dir, "test".into());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("whitelist"));
    }

    #[test]
    fn rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = create_skill_dir(tmp.path(), "ok.sh", "#!/bin/sh\necho hi");
        // Create a file outside skill dir
        fs::write(tmp.path().join("evil.sh"), "#!/bin/sh").unwrap();

        let def = make_def("bash", "../evil.sh");
        let result = SkillBundledTool::new(&def, skill_dir, "test".into());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("escapes skill directory"));
    }

    #[test]
    fn accepts_valid_tool() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = create_skill_dir(tmp.path(), "run.sh", "#!/bin/sh\necho '{}'");
        let def = make_def("bash", "scripts/run.sh");

        let tool = SkillBundledTool::new(&def, skill_dir, "test_skill".into());
        assert!(tool.is_ok());
        let tool = tool.unwrap();
        assert_eq!(tool.name(), "test_tool");
        assert_eq!(tool.description(), "A test tool");
    }

    #[tokio::test]
    async fn execute_sync_script() {
        let tmp = tempfile::tempdir().unwrap();
        let script = r#"#!/bin/sh
read input
echo '{"greeting": "hello"}'
"#;
        let skill_dir = create_skill_dir(tmp.path(), "greet.sh", script);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/greet.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let def = make_def("bash", "scripts/greet.sh");
        let tool = SkillBundledTool::new(&def, skill_dir, "test".into()).unwrap();

        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
        };
        let result = tool
            .execute(serde_json::json!({"name": "world"}), &ctx)
            .await;
        assert!(result.is_ok());
        match result.unwrap() {
            ToolOutput::Immediate(v) => {
                assert_eq!(v["greeting"], "hello");
            }
            _ => panic!("expected Immediate output"),
        }
    }

    #[tokio::test]
    async fn execute_nonzero_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\nexit 1\n";
        let skill_dir = create_skill_dir(tmp.path(), "fail.sh", script);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/fail.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let def = make_def("bash", "scripts/fail.sh");
        let tool = SkillBundledTool::new(&def, skill_dir, "test".into()).unwrap();

        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
        };
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("exited with code 1"));
    }

    #[tokio::test]
    async fn execute_async_job_protocol() {
        let tmp = tempfile::tempdir().unwrap();
        let script = r#"#!/bin/sh
if [ "$1" = "--poll" ]; then
    echo '{"status": "completed", "result": {"done": true}}'
else
    read input
    echo '{"__async_job": true, "job_id": "job_123", "poll_interval": 1}'
fi
"#;
        let skill_dir = create_skill_dir(tmp.path(), "async.sh", script);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/async.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let def = make_def("bash", "scripts/async.sh");
        let tool = SkillBundledTool::new(&def, skill_dir, "test".into()).unwrap();

        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
        };
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ToolOutput::AsyncJob(handle) => {
                assert_eq!(handle.job_id, "job_123");
                assert_eq!(handle.poll_interval, Duration::from_secs(1));

                // Poll the job
                let status = (handle.poll)().await;
                assert!(status.is_ok());
                match status.unwrap() {
                    JobStatus::Completed(v) => {
                        assert_eq!(v["done"], true);
                    }
                    other => panic!("expected Completed, got {:?}", other),
                }
            }
            _ => panic!("expected AsyncJob output"),
        }
    }
}
