# 001 · Code execution executor injection

## Background

`ExecutePythonTool` and `ExecuteJavaScriptTool` currently spawn bare subprocesses. Skill scripts already have a `ScriptExecutor` abstraction.

## Goal

Replace implicit bare-subprocess execution with explicit executor configuration for code execution tools.

## Acceptance Criteria

- [x] Code execution tools require an explicit executor configuration when code execution is enabled.
- [x] `BareSubprocessExecutor` remains available only as an explicitly selected development executor.
- [x] Code execution tools accept `Arc<dyn ScriptExecutor>`.
- [x] Executor-backed execution maps stdout/stderr/status consistently.
- [x] Missing executor configuration returns a structured configuration error before tool execution.
- [x] Tests cover bare-subprocess opt-in, injected-executor execution and missing-executor rejection.
- [x] Public examples are updated to configure an executor explicitly.
