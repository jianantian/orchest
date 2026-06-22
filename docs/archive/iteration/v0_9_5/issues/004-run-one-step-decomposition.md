# 004 · `run_one_step` decomposition

## Background

`run_one_step` mixes budget checks, hooks, model calls, tool execution, handoff handling and step finalization. The large function is difficult to review and already requires a clippy length suppression.

## Goal

Split `run_one_step` into named internal phases while preserving behavior.

## Acceptance Criteria

- [x] Limit and budget checks are extracted into a focused helper.
- [x] Model call with before/after hooks is extracted into a focused helper.
- [x] Tool execution loop is extracted or narrowed so approval/timeout/dispatch behavior is testable.
- [x] Handoff processing is extracted after issue 002 makes transition safety explicit.
- [x] `#[allow(clippy::too_many_lines)]` is removed from `run_one_step`.
- [x] Existing run-loop tests continue to pass.
- [x] No public API changes are introduced by the refactor.

## Notes

Keep helper visibility `pub(crate)` or private. Do not move logic into unrelated modules just to reduce line count.
