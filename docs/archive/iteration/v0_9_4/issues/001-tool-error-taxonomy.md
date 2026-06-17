# 001 · ToolError taxonomy

## Background

`ToolError` can currently express invalid input, unsupported operations, transient failures and fatal failures. The backlog identifies two missing classes: ambiguous requests and gaps in the SDK/application specification.

## Goal

Add `ErrorKind::Ambiguity` and `ErrorKind::SpecGap` with constructors, serialization tests and documentation.

## Acceptance Criteria

- [x] `crates/agent-runtime-core/src/tool/error.rs` defines `Ambiguity` and `SpecGap`.
- [x] `ToolError::ambiguity(message)` sets `kind = Ambiguity`, `retry = Unsafe` and `next_step = Some("clarify")`.
- [x] `ToolError::spec_gap(message)` sets `kind = SpecGap`, `retry = Unsafe` and `next_step = Some("escalate")`.
- [x] Existing `ToolError` serialization/deserialization tests are updated or added.
- [x] Public docs explain when to use `Ambiguity` vs `InvalidInput`.
- [x] `cargo test -p agent-runtime-core tool_error` passes.

## Notes

Do not change existing variant names or serde casing unless a migration plan is written first.
