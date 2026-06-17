# 004 · Repeated failure hook

## Background

The hook framework has lifecycle hooks, but repeated tool failure is only visible as isolated failures. Applications need a minimal pattern-detection hook to switch strategy, escalate or abort.

## Goal

Add a repeated-failure hook that triggers when the same tool fails with the same `ErrorKind` repeatedly within one run.

## Acceptance Criteria

- [x] A typed hook context includes `run_id`, `tool_name`, `error_kind`, `error_history` and `count`.
- [x] A `RepeatedFailureConfig` or equivalent runtime config field defines the threshold, with default threshold 3.
- [x] Runtime config rejects thresholds below 1.
- [x] Different `ErrorKind` values do not count as the same repeated failure pattern.
- [x] Hook action can continue or abort the run, matching existing hook action style where possible.
- [x] Tests prove the hook triggers after the default threshold, honors a custom threshold and does not trigger for mixed error kinds.
- [x] Public examples that configure runtime hooks are updated to show repeated-failure configuration when relevant.
- [x] Hook docs mention Supervised Delegation watcher use cases.

## Notes

Do not implement a full retro-agent or policy engine. This is only the hook point.
