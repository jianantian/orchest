# 004 · Optional parallel tool execution

## Background

Some model responses may request independent tool calls. Sequential execution is simpler and remains the default, but opt-in parallel execution can reduce latency.

## Goal

Add optional parallel tool execution with explicit safety constraints.

## Acceptance Criteria

- [ ] Parallel execution is disabled by default.
- [ ] Runtime config exposes an explicit parallelism setting with a conservative default.
- [ ] Tool metadata exposes `ToolParallelism::Serial` and `ToolParallelism::ParallelSafe`.
- [ ] Side-effecting, serial or approval-gated tools do not run concurrently.
- [ ] Budget, timeout and cancellation behavior are deterministic.
- [ ] Events include batch id, model-requested order and completion order metadata.
- [ ] Mixed serial/parallel batches execute deterministically: serial calls keep order, parallel-safe calls may overlap only within allowed groups.
- [ ] Tests cover parallel success, mixed failure, cancellation, mixed serial/parallel batches and approval-gated exclusion.
- [ ] Public examples show enabling parallel execution and marking a tool as parallel-safe.
