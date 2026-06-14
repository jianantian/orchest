# 001 · Code execution executor injection

## Background

`ExecutePythonTool` and `ExecuteJavaScriptTool` currently spawn bare subprocesses. Skill scripts already have a `ScriptExecutor` abstraction.

## Goal

Allow code execution tools to use an injected executor while preserving the current default.

## Acceptance Criteria

- [ ] Code execution tools accept `Option<Arc<dyn ScriptExecutor>>` or an equivalent executor abstraction.
- [ ] Default `None` behavior remains unchanged.
- [ ] Executor-backed execution maps stdout/stderr/status consistently.
- [ ] Tests cover default and injected-executor paths.
