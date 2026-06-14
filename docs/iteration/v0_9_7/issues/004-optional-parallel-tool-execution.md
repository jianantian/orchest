# 004 · Optional parallel tool execution

## Background

Some model responses may request independent tool calls. Sequential execution is simpler and remains the default, but opt-in parallel execution can reduce latency.

## Goal

Add optional parallel tool execution with explicit safety constraints.

## Acceptance Criteria

- [ ] Parallel execution is disabled by default.
- [ ] Side-effecting or approval-gated tools do not run concurrently unless explicitly safe.
- [ ] Budget, timeout and cancellation behavior are deterministic.
- [ ] Events preserve enough ordering metadata to reconstruct execution.
- [ ] Tests cover parallel success, mixed failure and cancellation.
