//! Code execution tools backed by an explicitly configured script executor.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::events::RuntimeEvent;
use crate::skill::executor::{ExecutionContext, ScriptError, ScriptExecutor, ScriptOutput};
use crate::skill::BundledToolDef;
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const MAX_TIMEOUT_SECONDS: u64 = 300;
const PYTHON_WRAPPER: &str = r#"
import contextlib
import io
import json
import sys
import traceback

payload = json.load(sys.stdin)
code = payload.get("code", "")
stdout = io.StringIO()
stderr = io.StringIO()
exit_code = 0

try:
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        exec(code, {})
except Exception:
    traceback.print_exc(file=stderr)
    exit_code = 1

sys.stdout.write(stdout.getvalue())
sys.stderr.write(stderr.getvalue())
raise SystemExit(exit_code)
"#;
const JAVASCRIPT_WRAPPER: &str = r#"
const fs = require("fs");
const payload = JSON.parse(fs.readFileSync(0, "utf8") || "{}");

(async () => {
  const result = await eval(payload.code || "");
  if (result !== undefined) {
    console.log(result);
  }
})().catch((error) => {
  console.error(error && error.stack ? error.stack : String(error));
  process.exitCode = 1;
});
"#;

struct CodeExecutionSpec {
    tool_name: &'static str,
    executable: &'static str,
    script_file_name: &'static str,
    wrapper: &'static str,
}

pub struct CodeExecutionMcpServer;

impl CodeExecutionMcpServer {
    pub fn tools(
        executor: Option<Arc<dyn ScriptExecutor>>,
    ) -> Result<Vec<Arc<dyn Tool>>, ToolError> {
        let executor = executor.ok_or_else(missing_executor_error)?;
        Ok(vec![
            Arc::new(ExecutePythonTool::new(Arc::clone(&executor))),
            Arc::new(ExecuteJavaScriptTool::new(executor)),
        ])
    }
}

fn missing_executor_error() -> ToolError {
    ToolError::fatal(
        "code execution is enabled but no ScriptExecutor is configured; inject an executor explicitly",
    )
    .with_code("CODE_EXECUTION_EXECUTOR_MISSING")
    .with_next_step("configure_code_execution_executor")
}

pub struct ExecutePythonTool {
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    executor: Arc<dyn ScriptExecutor>,
}

impl ExecutePythonTool {
    pub fn new(executor: Arc<dyn ScriptExecutor>) -> Self {
        Self {
            input_schema: code_input_schema(),
            metadata: code_tool_metadata(),
            executor,
        }
    }
}

#[async_trait]
impl Tool for ExecutePythonTool {
    fn name(&self) -> &str {
        "execute_python"
    }

    fn description(&self) -> &str {
        "Execute trusted Python code through the configured ScriptExecutor. Input: code and optional timeout_seconds."
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
        execute_with_script_executor(
            &*self.executor,
            CodeExecutionSpec {
                tool_name: "execute_python",
                executable: "python3",
                script_file_name: "orchest_code_exec.py",
                wrapper: PYTHON_WRAPPER,
            },
            input,
            ctx,
        )
        .await
    }
}

pub struct ExecuteJavaScriptTool {
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    executor: Arc<dyn ScriptExecutor>,
}

impl ExecuteJavaScriptTool {
    pub fn new(executor: Arc<dyn ScriptExecutor>) -> Self {
        Self {
            input_schema: code_input_schema(),
            metadata: code_tool_metadata(),
            executor,
        }
    }
}

#[async_trait]
impl Tool for ExecuteJavaScriptTool {
    fn name(&self) -> &str {
        "execute_javascript"
    }

    fn description(&self) -> &str {
        "Execute trusted JavaScript code through the configured ScriptExecutor. Input: code and optional timeout_seconds."
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
        execute_with_script_executor(
            &*self.executor,
            CodeExecutionSpec {
                tool_name: "execute_javascript",
                executable: "node",
                script_file_name: "orchest_code_exec.js",
                wrapper: JAVASCRIPT_WRAPPER,
            },
            input,
            ctx,
        )
        .await
    }
}

async fn execute_with_script_executor(
    executor: &dyn ScriptExecutor,
    spec: CodeExecutionSpec,
    input: Value,
    ctx: &ToolContext,
) -> Result<ToolOutput, ToolError> {
    let code = input.get("code").and_then(Value::as_str).ok_or_else(|| {
        ToolError::fatal(format!("{} input missing code", spec.tool_name))
            .with_code("INVALID_INPUT")
    })?;
    let timeout = requested_timeout(&input);
    let temp_dir = tempfile::tempdir().map_err(io_tool_error)?;
    let script_path = temp_dir.path().join(spec.script_file_name);
    tokio::fs::write(&script_path, spec.wrapper)
        .await
        .map_err(io_tool_error)?;

    let input_json = serde_json::to_vec(&json!({ "code": code })).map_err(|error| {
        ToolError::fatal(format!("failed to encode code execution input: {error}"))
            .with_code("SERIALIZATION_ERROR")
    })?;
    let tool_def = BundledToolDef {
        name: spec.tool_name.into(),
        description: String::new(),
        executable: spec.executable.into(),
        script: script_path.clone(),
        input_schema: code_input_schema(),
    };
    let exec_ctx = ExecutionContext {
        work_dir: temp_dir.path().to_path_buf(),
        env: code_execution_env(),
        capabilities: None,
        timeout: Some(timeout),
        on_update: None,
    };

    let output = match executor
        .execute(
            &tool_def,
            script_path.as_path(),
            &[],
            &input_json,
            &exec_ctx,
        )
        .await
    {
        Ok(output) => output,
        Err(error) if error.code.as_deref() == Some("TIMEOUT") => ScriptOutput {
            stdout: String::new(),
            stderr: "timeout".into(),
            exit_code: -1,
        },
        Err(error) => return Err(script_error_to_tool_error(error)),
    };

    emit_stdout_updates(ctx, spec.tool_name, &output.stdout).await;
    drop(temp_dir);
    Ok(ToolOutput::Immediate(json!({
        "stdout": output.stdout,
        "stderr": output.stderr,
        "exit_code": output.exit_code
    })))
}

fn code_input_schema() -> JsonSchema {
    json!({
        "type": "object",
        "properties": {
            "code": { "type": "string" },
            "timeout_seconds": { "type": "integer", "default": 30, "maximum": 300 }
        },
        "required": ["code"]
    })
}

fn code_tool_metadata() -> ToolMetadata {
    ToolMetadata {
        side_effect: true,
        approval: crate::tool::Approval::Never,
        timeout: Some(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS)),
        source: ToolSource::Builtin,
        ..ToolMetadata::default()
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

fn code_execution_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    if let Ok(path) = std::env::var("PATH") {
        env.insert("PATH".into(), path);
    }
    if let Ok(home) = std::env::var("HOME") {
        env.insert("HOME".into(), home);
    }
    env
}

async fn emit_stdout_updates(ctx: &ToolContext, tool: &str, stdout: &str) {
    if let Some(tx) = &ctx.event_tx {
        for stdout_line in stdout.lines() {
            let _ = tx
                .send(RuntimeEvent::ToolCallUpdate {
                    tool: tool.into(),
                    tool_call_id: ctx.tool_call_id.clone(),
                    partial: json!({"stdout_line": stdout_line}),
                })
                .await;
        }
    }
}

fn script_error_to_tool_error(error: ScriptError) -> ToolError {
    ToolError::fatal(format!("code execution executor failed: {}", error.message))
        .with_code(error.code.unwrap_or_else(|| "EXECUTOR_ERROR".into()))
}

fn io_tool_error(error: std::io::Error) -> ToolError {
    ToolError::transient(error.to_string()).with_code("IO_ERROR")
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::sync::Mutex;

    use crate::skill::executor::BareSubprocessExecutor;
    use tokio::sync::mpsc;

    fn tool_context(event_tx: Option<mpsc::Sender<RuntimeEvent>>) -> ToolContext {
        ToolContext {
            event_tx,
            ..ToolContext::oneshot()
        }
    }

    #[derive(Default)]
    struct FakeScriptExecutor {
        seen_executables: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ScriptExecutor for FakeScriptExecutor {
        async fn execute(
            &self,
            tool: &BundledToolDef,
            script_path: &Path,
            _args: &[String],
            input_json: &[u8],
            _ctx: &ExecutionContext,
        ) -> Result<ScriptOutput, ScriptError> {
            assert!(script_path.exists());
            let input: Value = serde_json::from_slice(input_json).expect("input json");
            self.seen_executables
                .lock()
                .expect("lock")
                .push(tool.executable.clone());
            Ok(ScriptOutput {
                stdout: format!("ran {}", input["code"].as_str().unwrap_or_default()),
                stderr: String::new(),
                exit_code: 0,
            })
        }
    }

    #[test]
    fn tools_missing_executor_returns_configuration_error() {
        let error = match CodeExecutionMcpServer::tools(None) {
            Ok(_) => panic!("executor is required"),
            Err(error) => error,
        };

        assert_eq!(
            error.code.as_deref(),
            Some("CODE_EXECUTION_EXECUTOR_MISSING")
        );
    }

    #[tokio::test]
    async fn python_tool_uses_injected_executor() {
        let executor = Arc::new(FakeScriptExecutor::default());
        let tool = ExecutePythonTool::new(executor.clone());
        let output = tool
            .execute(json!({"code": "print(1)"}), &tool_context(None))
            .await
            .expect("tool executes");

        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        assert_eq!(value["stdout"], "ran print(1)");
        assert_eq!(
            executor.seen_executables.lock().expect("lock").as_slice(),
            ["python3"]
        );
    }

    #[tokio::test]
    async fn bare_subprocess_executor_is_explicit_opt_in() {
        let tools = CodeExecutionMcpServer::tools(Some(Arc::new(BareSubprocessExecutor::new())))
            .expect("tools");
        let python = tools
            .iter()
            .find(|tool| tool.name() == "execute_python")
            .expect("python tool");
        let (event_tx, mut event_rx) = mpsc::channel(16);

        let output = python
            .execute(
                json!({"code": "print(41)", "timeout_seconds": 5}),
                &tool_context(Some(event_tx)),
            )
            .await
            .expect("python executes");

        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        assert_eq!(value["stdout"], "41\n");
        assert_eq!(value["stderr"], "");
        assert_eq!(value["exit_code"], 0);
        let event = event_rx.recv().await.expect("stdout update");
        assert!(matches!(
            event,
            RuntimeEvent::ToolCallUpdate { partial, .. } if partial["stdout_line"] == "41"
        ));
    }

    #[tokio::test]
    async fn executor_timeout_maps_to_status_payload() {
        struct TimeoutExecutor;

        #[async_trait]
        impl ScriptExecutor for TimeoutExecutor {
            async fn execute(
                &self,
                _tool: &BundledToolDef,
                _script_path: &Path,
                _args: &[String],
                _input_json: &[u8],
                _ctx: &ExecutionContext,
            ) -> Result<ScriptOutput, ScriptError> {
                Err(ScriptError {
                    message: "script execution timed out".into(),
                    code: Some("TIMEOUT".into()),
                })
            }
        }

        let tool = ExecuteJavaScriptTool::new(Arc::new(TimeoutExecutor));
        let output = tool
            .execute(json!({"code": "while (true) {}"}), &tool_context(None))
            .await
            .expect("timeout is a structured output");

        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        assert_eq!(value["stdout"], "");
        assert_eq!(value["stderr"], "timeout");
        assert_eq!(value["exit_code"], -1);
    }
}
