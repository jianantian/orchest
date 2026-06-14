# 001 · Deprecated API removal

## Background

Several APIs were deprecated in v0.9.0 and should be removed before the v1.0 API freeze if migration is complete.

## Goal

Remove deprecated APIs and update bindings/tests/docs.

## Acceptance Criteria

- [ ] `as_tool_legacy` is removed or has a documented reason to remain.
- [ ] `ApprovalMode::SideEffectOnly` is removed or has a documented reason to remain.
- [ ] `AgentAsTool::new` is removed or has a documented reason to remain.
- [ ] Python and Node bindings use current APIs.
- [ ] Migration notes are added to docs.
