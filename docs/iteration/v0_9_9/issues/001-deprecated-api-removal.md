# 001 · Deprecated API removal

## Background

Several APIs were deprecated in v0.9.0 and should be removed in the pre-v1.0 breaking-change window.

## Goal

Remove deprecated APIs and update bindings/tests/docs.

## Acceptance Criteria

- [ ] `as_tool_legacy` is removed.
- [ ] `ApprovalMode::SideEffectOnly` is removed from core config and serde/binding parsing.
- [ ] `AgentAsTool::new` is removed; construction goes through `AgentConfig::as_tool(...).build()`.
- [ ] Python `requires_approval` aliases are removed; callers use `approval`.
- [ ] Python `side_effect_only` approval mode parsing is removed.
- [ ] Node `requiresApproval` aliases are removed; callers use `approval`.
- [ ] Node `sideEffectOnly` approval mode parsing is removed.
- [ ] Tests that used `#[allow(deprecated)]` are rewritten to current APIs.
- [ ] Public examples and migration notes are updated.

## Notes

Do not keep deprecated APIs for source compatibility. If an item cannot be removed because it is still the only way to express a required behavior, create a replacement API in this issue and remove the deprecated item after replacing its usage.
