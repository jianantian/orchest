# 002 · Draft/Commit approval behavior

## Background

Draft/Commit only works if dispatch and approval behavior are predictable.

## Goal

Make draft calls side-effect-free by default and commit calls approval-gated by default.

## Acceptance Criteria

- [ ] Draft tool calls default to no approval and no side effect.
- [ ] Commit tool calls default to approval-required.
- [ ] Approval denial prevents commit execution.
- [ ] Runtime events make draft vs commit visible enough for debugging.
- [ ] Tests cover draft success, commit approval, and commit denial.
