# 002 · Structured tool failure return

## Background

The run loop currently returns only `error.message` to the model for failed tool calls. That drops `kind`, `retry`, `code` and `next_step`, making ambiguity, transient failure and spec-gap handling indistinguishable to the model.

## Goal

Return structured tool error payloads to the model while preserving existing event behavior.

## Acceptance Criteria

- [ ] Tool result content for failures includes an `error` object with `message`, `kind`, `retry`, `code` and `next_step`.
- [ ] `RuntimeEvent::ToolCallFailed` continues to carry the full `ToolError`.
- [ ] Existing tests that assert error shape are updated intentionally.
- [ ] A focused run-loop test verifies that a `ToolError::spec_gap("missing contract")` reaches the model-facing tool result with `kind = "SpecGap"` or the established serde equivalent.
- [ ] No private fields are exposed across FFI boundaries without explicit conversion.

## Notes

Keep compatibility in mind: if callers previously expected `{"error": "...message..."}`, document the shape change in the issue implementation notes.
