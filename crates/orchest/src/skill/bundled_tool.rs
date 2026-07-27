//! Bundled tool wrapper: executes skill scripts as tool implementations.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::skill::executor::{BareSubprocessExecutor, ExecutionContext, ScriptExecutor};
use crate::skill::{
    BundledToolDef, CapabilityValidator, SkillCapabilities, SkillDependencies, SkillEnvManager,
};
use crate::tool::async_job::{JobHandle, JobStatus};
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

const ALLOWED_EXECUTABLES: &[&str] = &["python", "python3", "node", "bash", "sh"];

pub struct SkillBundledTool {
    name: String,
    description: String,
    executable: String,
    script: PathBuf,
    skill_dir: PathBuf,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    dependencies: SkillDependencies,
    capabilities: Option<SkillCapabilities>,
    executor: Arc<dyn ScriptExecutor>,
    env_manager: SkillEnvManager,
}

impl std::fmt::Debug for SkillBundledTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillBundledTool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("executable", &self.executable)
            .field("script", &self.script)
            .field("skill_dir", &self.skill_dir)
            .field("input_schema", &self.input_schema)
            .field("metadata", &self.metadata)
            .field("dependencies", &self.dependencies)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl SkillBundledTool {
    pub fn new(
        def: &BundledToolDef,
        skill_dir: PathBuf,
        skill_name: String,
    ) -> Result<Self, ToolError> {
        Self::new_with_options(
            def,
            skill_dir,
            skill_name,
            SkillDependencies::default(),
            None,
            Arc::new(BareSubprocessExecutor::new()),
        )
    }

    #[allow(clippy::too_many_arguments)] // justified: skill tool needs definition, paths, deps, capabilities, and executor
    pub fn new_with_options(
        def: &BundledToolDef,
        skill_dir: PathBuf,
        skill_name: String,
        dependencies: SkillDependencies,
        capabilities: Option<SkillCapabilities>,
        executor: Arc<dyn ScriptExecutor>,
    ) -> Result<Self, ToolError> {
        if !ALLOWED_EXECUTABLES.contains(&def.executable.as_str()) {
            return Err(ToolError::fatal(format!(
                "executable '{}' not in whitelist: {:?}",
                def.executable, ALLOWED_EXECUTABLES
            ))
            .with_code("INVALID_EXECUTABLE"));
        }

        let script_abs = skill_dir.join(&def.script);
        let script_resolved = script_abs.canonicalize().map_err(|e| {
            ToolError::fatal(format!(
                "failed to resolve script path '{}': {}",
                def.script.display(),
                e
            ))
            .with_code("SCRIPT_NOT_FOUND")
        })?;

        let skill_dir_resolved = skill_dir.canonicalize().map_err(|e| {
            ToolError::fatal(format!(
                "failed to resolve skill dir '{}': {}",
                skill_dir.display(),
                e
            ))
            .with_code("SKILL_DIR_ERROR")
        })?;

        if !script_resolved.starts_with(&skill_dir_resolved) {
            return Err(ToolError::fatal(format!(
                "script path '{}' escapes skill directory '{}'",
                def.script.display(),
                skill_dir.display()
            ))
            .with_code("PATH_TRAVERSAL"));
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
                approval: crate::tool::Approval::Never,
                source: ToolSource::Skill { skill_name },
                ..ToolMetadata::default()
            },
            dependencies,
            capabilities,
            executor,
            env_manager: SkillEnvManager::default(),
        })
    }

    /// Owning skill's name for log/event context; falls back to the tool
    /// name for non-skill sources (defensive — bundled tools are always
    /// skill-sourced).
    fn skill_name(&self) -> &str {
        match &self.metadata.source {
            ToolSource::Skill { skill_name } => skill_name,
            _ => &self.name,
        }
    }

    #[allow(clippy::too_many_arguments)] // justified: script execution needs IO + context params; internal method
    async fn spawn_script(
        &self,
        input_json: &[u8],
        extra_args: &[String],
        timeout: Option<Duration>,
        parent_run_id: Option<crate::run::RunId>,
        run_depth: u32,
    ) -> Result<(String, String, i32), ToolError> {
        let tempdir = tempfile::tempdir().map_err(|e| {
            ToolError::fatal(format!("failed to create temporary work dir: {e}"))
                .with_code("TEMP_DIR_ERROR")
        })?;
        let mut env = CapabilityValidator::execution_env(self.capabilities.as_ref());
        if let Some(parent_run_id) = parent_run_id {
            env.insert("ORCHEST_PARENT_RUN_ID".into(), parent_run_id.to_string());
        }
        env.insert("ORCHEST_RUN_DEPTH".into(), run_depth.to_string());
        let mut executable = self.executable.clone();
        let manifest_for_env = crate::skill::SkillManifest {
            name: self.skill_name().to_string(),
            description: self.description.clone(),
            path: self.skill_dir.clone(),
            // Synthetic manifest for env management only; the scanner's
            // manifest filename is not tracked on this path.
            skill_md_path: PathBuf::new(),
            allowed_tools: None,
            bundled_tools: vec![],
            dependencies: self.dependencies.clone(),
            capabilities: self.capabilities.clone(),
            raw_frontmatter: Value::Null,
        };

        if matches!(self.executable.as_str(), "python" | "python3")
            && !self.dependencies.python.is_empty()
        {
            let env_dir = self
                .env_manager
                .ensure_python_env(&manifest_for_env)
                .await
                .map_err(|e| {
                    let mut err = ToolError::fatal(e.message);
                    err.code = e.code;
                    err
                })?;
            executable = env_dir
                .join("bin")
                .join("python")
                .to_string_lossy()
                .to_string();
        }

        if self.executable == "node" && !self.dependencies.node.is_empty() {
            let env_dir = self
                .env_manager
                .ensure_node_env(&manifest_for_env)
                .await
                .map_err(|e| {
                    let mut err = ToolError::fatal(e.message);
                    err.code = e.code;
                    err
                })?;
            let sdk_dir = self
                .env_manager
                .ensure_builtin_node_sdk()
                .await
                .map_err(|e| {
                    let mut err = ToolError::fatal(e.message);
                    err.code = e.code;
                    err
                })?;
            env.insert(
                "NODE_PATH".into(),
                format!(
                    "{}:{}",
                    env_dir.join("node_modules").to_string_lossy(),
                    sdk_dir.to_string_lossy()
                ),
            );
        } else if self.executable == "node" {
            let sdk_dir = self
                .env_manager
                .ensure_builtin_node_sdk()
                .await
                .map_err(|e| {
                    let mut err = ToolError::fatal(e.message);
                    err.code = e.code;
                    err
                })?;
            env.insert("NODE_PATH".into(), sdk_dir.to_string_lossy().to_string());
        }

        let tool = BundledToolDef {
            name: self.name.clone(),
            description: self.description.clone(),
            executable,
            script: self.script.clone(),
            input_schema: self.input_schema.clone(),
        };
        let ctx = ExecutionContext {
            work_dir: tempdir.path().to_path_buf(),
            env,
            capabilities: self.capabilities.clone(),
            timeout,
            on_update: None,
        };
        let output = self
            .executor
            .execute(&tool, &self.script, extra_args, input_json, &ctx)
            .await
            .map_err(|e| {
                let mut err = ToolError::fatal(e.message);
                err.code = e.code;
                err
            })?;
        Ok((output.stdout, output.stderr, output.exit_code))
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

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let input_json = serde_json::to_vec(&input).map_err(|e| {
            ToolError::fatal(format!("failed to serialize input: {}", e))
                .with_code("SERIALIZATION_ERROR")
        })?;

        let (stdout, stderr, exit_code) = self
            .spawn_script(
                &input_json,
                &[],
                self.metadata.timeout,
                Some(ctx.run_id),
                ctx.run_depth,
            )
            .await?;

        if !stderr.is_empty() {
            tracing::warn!(
                skill = %self.skill_name(),
                tool = %self.name,
                stderr = %stderr.trim(),
                "skill script wrote to stderr"
            );
        }

        if exit_code != 0 {
            return Err(ToolError::fatal(format!(
                "script exited with code {}: {}",
                exit_code,
                stderr.trim()
            ))
            .with_code("NON_ZERO_EXIT"));
        }

        let parsed: Value = serde_json::from_str(stdout.trim()).map_err(|e| {
            ToolError::fatal(format!("failed to parse script stdout as JSON: {}", e))
                .with_code("INVALID_OUTPUT")
        })?;

        if parsed.get("__async_job").and_then(|v| v.as_bool()) == Some(true) {
            let job_id = parsed
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::fatal("async job response missing 'job_id'")
                        .with_code("MISSING_JOB_ID")
                })?
                .to_string();

            let poll_interval_secs = parsed
                .get("poll_interval")
                .and_then(|v| v.as_u64())
                .unwrap_or(5);

            let tool = Arc::new(self.clone_for_poll());
            let tool_name = self.name.clone();
            let poll_job_id = job_id.clone();

            let poll_fn = move || {
                let tool = Arc::clone(&tool);
                let tool_name = tool_name.clone();
                let poll_job_id = poll_job_id.clone();

                Box::pin(async move {
                    let (stdout, stderr, exit_code) = tool
                        .spawn_script(
                            &[],
                            &["--poll".to_string(), poll_job_id.clone()],
                            tool.metadata.timeout,
                            None,
                            0,
                        )
                        .await?;

                    if !stderr.is_empty() {
                        tracing::warn!(
                            skill = %tool.skill_name(),
                            tool = %tool_name,
                            poll = true,
                            stderr = %stderr.trim(),
                            "skill script wrote to stderr"
                        );
                    }

                    if exit_code != 0 {
                        return Err(ToolError::fatal(format!(
                            "poll script exited with code {}: {}",
                            exit_code,
                            stderr.trim()
                        ))
                        .with_code("NON_ZERO_EXIT"));
                    }

                    let parsed: Value = serde_json::from_str(stdout.trim()).map_err(|e| {
                        ToolError::fatal(format!("failed to parse poll output: {}", e))
                            .with_code("INVALID_OUTPUT")
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
                poll: Some(Arc::new(poll_fn)),
                poll_interval: Duration::from_secs(poll_interval_secs),
                timeout: self.metadata.timeout,
                webhook: None,
            }));
        }

        Ok(ToolOutput::Immediate(parsed))
    }
}

impl SkillBundledTool {
    fn clone_for_poll(&self) -> Self {
        Self {
            name: self.name.clone(),
            description: self.description.clone(),
            executable: self.executable.clone(),
            script: self.script.clone(),
            skill_dir: self.skill_dir.clone(),
            input_schema: self.input_schema.clone(),
            metadata: self.metadata.clone(),
            dependencies: self.dependencies.clone(),
            capabilities: self.capabilities.clone(),
            executor: Arc::clone(&self.executor),
            env_manager: self.env_manager.clone(),
        }
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

        let ctx = ToolContext::oneshot();
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

        let ctx = ToolContext::oneshot();
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

        let ctx = ToolContext::oneshot();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ToolOutput::AsyncJob(handle) => {
                assert_eq!(handle.job_id, "job_123");
                assert_eq!(handle.poll_interval, Duration::from_secs(1));

                // Poll the job
                let status = (handle.poll.as_ref().expect("poll should exist"))().await;
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

    #[tokio::test]
    async fn execute_routes_script_stderr_to_tracing() {
        use std::io::Write;
        use std::sync::Mutex;

        // In-memory MakeWriter capturing everything the subscriber writes.
        #[derive(Clone, Default)]
        struct SharedBuf(Arc<Mutex<Vec<u8>>>);
        impl Write for SharedBuf {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buf = SharedBuf::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer({
                let buf = buf.clone();
                move || buf.clone()
            })
            .with_ansi(false)
            .finish();
        // Thread-local default subscriber; #[tokio::test] runs on the
        // current thread, so the guard covers the awaits below.
        let _guard = tracing::subscriber::set_default(subscriber);

        let tmp = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\necho 'some warning output' >&2\necho '{\"ok\": true}'\n";
        let skill_dir = create_skill_dir(tmp.path(), "noisy.sh", script);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/noisy.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let def = make_def("bash", "scripts/noisy.sh");
        let tool = SkillBundledTool::new(&def, skill_dir, "noisy_skill".into()).unwrap();

        let ctx = ToolContext::oneshot();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_ok());

        let logs = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(
            logs.contains("some warning output"),
            "tracing should capture the script stderr, got: {logs}"
        );
        assert!(logs.contains("WARN"), "expected WARN level, got: {logs}");
        assert!(
            logs.contains("noisy_skill") && logs.contains("test_tool"),
            "expected skill/tool context fields, got: {logs}"
        );
    }

    #[tokio::test]
    async fn execution_context_does_not_inherit_parent_env() {
        std::env::set_var("ORCHEST_VISIBLE_ENV", "allowed");
        std::env::set_var("ORCHEST_HIDDEN_ENV", "blocked");
        let tmp = tempfile::tempdir().unwrap();
        let script = r#"#!/bin/sh
echo "{\"visible\":\"$ORCHEST_VISIBLE_ENV\",\"hidden\":\"$ORCHEST_HIDDEN_ENV\",\"cwd\":\"$(pwd)\"}"
"#;
        let skill_dir = create_skill_dir(tmp.path(), "env.sh", script);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                skill_dir.join("scripts/env.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let def = make_def("bash", "scripts/env.sh");
        let capabilities = crate::skill::SkillCapabilities {
            network: false,
            filesystem_read: vec![],
            filesystem_write: vec![],
            env: vec!["ORCHEST_VISIBLE_ENV".into()],
            max_memory_mb: None,
        };
        let tool = SkillBundledTool::new_with_options(
            &def,
            skill_dir.clone(),
            "test".into(),
            crate::skill::SkillDependencies::default(),
            Some(capabilities),
            Arc::new(crate::skill::executor::BareSubprocessExecutor::new()),
        )
        .unwrap();

        let ctx = ToolContext::oneshot();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        let ToolOutput::Immediate(value) = result else {
            panic!("expected immediate output");
        };
        assert_eq!(value["visible"], "allowed");
        assert_eq!(value["hidden"], "");
        assert_ne!(
            value["cwd"].as_str(),
            Some(skill_dir.to_string_lossy().as_ref())
        );
    }
}
