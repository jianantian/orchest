# 001 · Explicit sub-agent ContextMode

## Background

`SubAgentBuilder` currently encodes context inheritance through a numeric `inherit_context_count`. `0` and `Some(n)` carry meaningful semantics, but the API does not name those semantics.

## Goal

Replace the numeric context inheritance API with explicit `ContextMode::Fresh | Fork { depth }` semantics for Agent-as-Tool/sub-agent execution.

## Acceptance Criteria

- [x] Public `ContextMode` type exists in the appropriate core module.
- [x] `Fresh` starts the child run without parent history.
- [x] `Fork { depth }` inherits exactly the latest `depth` parent messages.
- [x] `Fork` depth is represented as non-zero by type where practical, or `depth == 0` is rejected during builder validation.
- [x] `Fork { depth }` with no available parent messages fails loudly or produces a structured error; it must not silently degrade to `Fresh`.
- [x] `inherit_context_count` is removed from public configuration/state.
- [x] The `inherit_context(...)` builder helper is removed; callers use `context_mode(ContextMode::...)`.
- [x] Tests cover fresh, fork depth, zero-depth rejection and empty-parent fork behavior.
- [x] Public examples are updated to use `ContextMode`.

## Notes

Prefer naming that fits existing `AgentAsTool` / `SubAgentBuilder` public API style. Do not preserve the old helper API.
