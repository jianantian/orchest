# 004 · Optional parallel tool execution

## Background

Some model responses may request independent tool calls. Sequential execution is simpler and remains the default, but opt-in parallel execution can reduce latency.

## Goal

Add optional parallel tool execution with explicit safety constraints.

## Acceptance Criteria

- [x] Parallel execution is disabled by default.
- [x] Runtime config exposes an explicit parallelism setting with a conservative default.
- [x] Tool metadata exposes `ToolParallelism::Serial` and `ToolParallelism::ParallelSafe`.
- [x] Side-effecting, serial or approval-gated tools do not run concurrently.
- [x] Budget, timeout and cancellation behavior are deterministic.
- [x] Events include batch id, model-requested order and completion order metadata.
- [x] Mixed serial/parallel batches execute deterministically: serial calls keep order, parallel-safe calls may overlap only within allowed groups.
- [x] Tests cover parallel success, mixed failure, cancellation, mixed serial/parallel batches and approval-gated exclusion.
- [x] Public examples show enabling parallel execution and marking a tool as parallel-safe.

## Notes

The initial runtime policy is conservative: a same-turn batch runs concurrently
only when every call is side-effect-free, approval-free, marked
`ToolParallelism::ParallelSafe`, and no hooks or retry policy are active. Mixed
serial/parallel batches fall back to the existing ordered execution path.
