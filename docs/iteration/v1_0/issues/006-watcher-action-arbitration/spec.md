# 006 · Deterministic multi-watcher action arbitration

GitHub issue: #254

## Background

SB-7 records that each watcher runs in an independent task. Registration
order does not establish an execution order for concurrent `Inject`, `Steer`,
or `Abort` actions.

## Goal

Define and implement a deterministic, documented arbitration contract for
actions returned by multiple watchers.

## Acceptance Criteria

- [ ] Conflicting actions for the same event resolve according to one public,
  deterministic rule.
- [ ] The rule covers `Continue`, `Inject`, `Steer`, and `Abort`.
- [ ] Slow watcher completion cannot make the result nondeterministic.
- [ ] Per-watcher FIFO and no-drop sequence tests remain distinct from action
  arbitration tests.
- [ ] Tests prove the same effective action order across repeated runs.

## Notes

This is a v1.0 pre-freeze gate.
