# 001 · Code execution executor injection

## Background

`ExecutePythonTool` and `ExecuteJavaScriptTool` currently spawn bare subprocesses. Skill scripts already have a `ScriptExecutor` abstraction.

## Goal

Replace implicit bare-subprocess execution with explicit executor configuration for code execution tools.

## Acceptance Criteria

- [ ] Code execution tools require an explicit executor configuration when code execution is enabled.
- [ ] `BareSubprocessExecutor` remains available only as an explicitly selected development executor.
- [ ] Code execution tools accept `Arc<dyn ScriptExecutor>`.
- [ ] Executor-backed execution maps stdout/stderr/status consistently.
- [ ] Missing executor configuration returns a structured configuration error before tool execution.
- [ ] Tests cover bare-subprocess opt-in, injected-executor execution and missing-executor rejection.
- [ ] Public examples are updated to configure an executor explicitly.
