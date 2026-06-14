# 001 · Explicit sub-agent ContextMode

## Background

`SubAgentBuilder` currently encodes context inheritance through a numeric `inherit_context_count`. `0` and `Some(n)` carry meaningful semantics, but the API does not name those semantics.

## Goal

Introduce explicit `ContextMode::Fresh | Fork { depth }` semantics for Agent-as-Tool/sub-agent execution.

## Acceptance Criteria

- [ ] Public `ContextMode` type exists in the appropriate core module.
- [ ] `Fresh` starts the child run without parent history.
- [ ] `Fork { depth }` inherits exactly the latest `depth` parent messages.
- [ ] `Fork { depth }` with no available parent messages fails loudly or produces a structured error; it must not silently degrade to `Fresh`.
- [ ] Existing builder helpers remain source-compatible or have a clear deprecation/migration note.
- [ ] Tests cover fresh, fork depth and empty-parent fork behavior.

## Notes

Prefer naming that fits existing `AgentAsTool` / `SubAgentBuilder` public API style.
