# 002 · Draft/Commit approval behavior

## Background

Draft/Commit only works if dispatch and approval behavior are predictable.

## Goal

Make draft calls side-effect-free by default and commit calls approval-gated by default.

## Acceptance Criteria

- [x] Draft tool calls default to no approval and no side effect.
- [x] Commit tool calls default to approval-required.
- [x] Commit approval includes a reason/context that identifies the linked draft when available.
- [x] Approval denial prevents commit execution.
- [x] Run-level permissive approval settings do not accidentally bypass commit approval unless a custom approval function explicitly does so.
- [x] Runtime events make draft vs commit visible enough for debugging.
- [x] Tests cover draft success, commit approval, commit denial and attempted commit approval bypass.
- [x] Public examples show a high-risk tool pair using draft output as commit input.
