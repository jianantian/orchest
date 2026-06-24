# v0.9.8 PRD: Runtime Safety and Observability Hygiene

## Background

Code review identified real runtime hygiene work that should not stay as informal notes: code execution sandbox injection, observability gaps, binding behavior documentation and small allocation cleanups.

## Goals

1. Add a sandbox/executor injection point to code execution tools.
2. Expand core runtime observability for model calls, budget, approval, compaction and event backpressure.
3. Document or improve Python GIL behavior.
4. Clean up low-risk static error payload allocations.

## Non-Goals

- No full sandbox implementation in core.
- No hosted observability platform.
- No binding crate architecture split; that is v0.9.9.

## Scope

### Code Execution Executor

Replace hidden bare-subprocess spawning inside code execution tools with an explicit executor configuration. `BareSubprocessExecutor` may remain available as a development executor, but applications must choose it explicitly; do not silently fall back to it from an absent executor.

The code execution tools must use `Arc<dyn ScriptExecutor>`. Do not introduce a second code-exec-specific executor trait in this iteration.

### Observability

Core telemetry must cover model calls, token usage, budget utilization, approval latency, compaction and event channel pressure with stable metric/span names documented in the observability guide.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Code execution executor injection | Replace implicit bare subprocess execution with explicit executor configuration |
| 002 | Core observability metrics | Model duration/tokens, budget, approval latency, compaction, event drops |
| 003 | Python GIL behavior | Verify current behavior, document constraints, improve if needed |
| 004 | Static error payload cleanup | Replace low-value repeated `json!` allocations where simple |

## Acceptance Criteria

- [x] Code execution tools run through an explicit executor configuration; bare subprocess execution is opt-in and visible.
- [x] Core telemetry covers the listed observability gaps.
- [x] Python SDK docs accurately describe GIL behavior.
- [ ] Static error payload cleanup preserves the post-v0.9.4 structured model-facing error shape.
- [x] Public examples and tests are updated for explicit code execution executor configuration and new telemetry names.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.
