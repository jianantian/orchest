# 004 · Repeated failure hook

## Background

The hook framework has lifecycle hooks, but repeated tool failure is only visible as isolated failures. Applications need a minimal pattern-detection hook to switch strategy, escalate or abort.

## Goal

Add a repeated-failure hook that triggers when the same tool fails with the same `ErrorKind` repeatedly within one run.

## Acceptance Criteria

- [ ] A typed hook context includes `run_id`, `tool_name`, `error_kind`, `error_history` and `count`.
- [ ] The threshold is configurable, with a conservative default.
- [ ] Different `ErrorKind` values do not count as the same repeated failure pattern.
- [ ] Hook action can continue or abort the run, matching existing hook action style where possible.
- [ ] Tests prove the hook triggers after the threshold and does not trigger for mixed error kinds.
- [ ] Hook docs mention Supervised Delegation watcher use cases.

## Notes

Do not implement a full retro-agent or policy engine. This is only the hook point.
