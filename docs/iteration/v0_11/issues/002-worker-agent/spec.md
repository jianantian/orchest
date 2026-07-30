# 002 · Worker, context, and fault primitives

## Background

The delegated worker needs deterministic research tools and a controlled
failure path. A fatal unsafe `ToolError` is only a tool result; it does not
terminate the run by itself.

## Goal

Implement the worker component, `ContextMode` fixtures, and fault primitives.
Collect source and executed-test evidence in `findings.json`.

## Acceptance Criteria

- [ ] `src/worker.rs` constructs a worker with `search_corpus`, `read_file`,
  `write_draft`, and `fault_trigger`.
- [ ] `write_draft` declares its side-effect metadata.
- [ ] `fault_trigger` returns a structured fatal error with
  `RetryHint::Unsafe`.
- [ ] The worker config sets `repeated_failure_threshold(1)`.
- [ ] A `Hook::on_repeated_failure` implementation returns
  `HookAction::Abort` for the controlled fatal error.
- [ ] A deterministic test proves the chain ends in worker `RunFailed`.
- [ ] Tests distinguish the tool error from the hook-driven termination; no
  text claims the tool error alone terminates the run.
- [ ] `ContextMode::Fresh`, `Fork { depth }`, and fork-without-history behavior
  have deterministic coverage.
- [ ] Owned checklist and finding records in `findings.json` reference actual
  source or executed test evidence.
- [ ] The worker remains a delegated library component, not a separate CLI.

## Boundary

This issue does not claim supervisor restart or worker recovery. Supervisor
escalation and restart evidence belong to issue 004.
