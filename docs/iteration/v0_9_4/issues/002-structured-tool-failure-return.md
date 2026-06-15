# 002 · Structured tool failure return

## Background

The run loop currently returns only `error.message` to the model for failed tool calls. That drops `kind`, `retry`, `code` and `next_step`, making ambiguity, transient failure and spec-gap handling indistinguishable to the model.

## Goal

Replace the old string-only model-facing tool error payload with a structured error object while preserving existing event behavior.

## Acceptance Criteria

- [x] Tool result content for failures includes an `error` object with `message`, `kind`, `retry`, `code` and `next_step`.
- [x] `RuntimeEvent::ToolCallFailed` continues to carry the full `ToolError`.
- [x] Existing tests and examples that assert or show error shape are updated to the structured `error` object.
- [x] A focused run-loop test verifies that a `ToolError::spec_gap("missing contract")` reaches the model-facing tool result with `kind = "SpecGap"` or the established serde equivalent.
- [x] No legacy model-facing shape with a bare string in `error` remains in docs, examples or tests.
- [x] No private fields are exposed across FFI boundaries without explicit conversion.

## Notes

This is an intentional breaking cleanup. Do not add compatibility adapters for the old string-only shape.
