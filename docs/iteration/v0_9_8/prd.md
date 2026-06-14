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

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Code execution executor injection | Let code exec tools use `ScriptExecutor` or equivalent |
| 002 | Core observability metrics | Model duration/tokens, budget, approval latency, compaction, event drops |
| 003 | Python GIL behavior | Verify current behavior, document constraints, improve if needed |
| 004 | Static error payload cleanup | Replace low-value repeated `json!` allocations where simple |

## Acceptance Criteria

- [ ] Code execution tools can run through an injected executor without changing default behavior.
- [ ] Core telemetry covers the listed observability gaps.
- [ ] Python SDK docs accurately describe GIL behavior.
- [ ] Static error payload cleanup does not change model-facing behavior.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.
