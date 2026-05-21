# 007 · Fix MCP Retry and Child Process Timeout Behavior

## Background

Two reliability risks remain in long-running or side-effectful execution paths:

- MCP HTTP retries all methods, including `tools/call`, which can duplicate side effects after timeouts or transient network failures
- subprocess/code-exec timeout branches can return timeout errors while leaving the child process running

## Goal

Make failure handling conservative for side effects and ensure timed-out child processes are cleaned up.

## Acceptance Criteria

**MCP HTTP retry safety:**
- [ ] `initialize` and `tools/list` may retry because they are discovery/control operations
- [ ] `tools/call` does not retry by default
- [ ] If retry for `tools/call` is ever enabled, it requires an explicit idempotency signal from tool metadata or config
- [ ] A test simulates first-request timeout/connection close and verifies `tools/call` is sent once

**MCP process lifecycle:**
- [ ] Stdio MCP child process is terminated when the client is dropped, unless documented otherwise
- [ ] Tests do not leave MCP child processes behind

**Skill subprocess timeout cleanup:**
- [ ] `BareSubprocessExecutor` kills the child process on timeout
- [ ] Timeout cleanup is covered by a test using a long-running script
- [ ] The test verifies the process is not still alive after timeout where feasible

**Code execution timeout cleanup:**
- [ ] JavaScript execution kills child runtime on timeout
- [ ] Python persistent session is killed and reset on timeout, preserving existing behavior
- [ ] Follow-up calls after timeout create a fresh working session

## Notes

Use conservative defaults. A side-effectful remote call that may or may not have executed should not be retried automatically without an idempotency key.

**Current status:** As of this writing, the codebase contains no retry logic for MCP HTTP calls. The acceptance criteria here are partly preventive — ensuring that when retry is added (e.g., for `initialize` and `tools/list`), `tools/call` is explicitly excluded by default. Tests should still verify the single-attempt behavior to prevent future regressions.
