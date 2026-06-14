# 003 · Deferred tool discovery refresh

## Background

Earlier iterations introduced a `search_tools` concept. The current runtime needs a refreshed contract for large tool libraries and model adapter schema injection.

## Goal

Align deferred tool discovery with the current ToolRegistry and run-level tool lifecycle.

## Acceptance Criteria

- [ ] Current `search_tools` behavior is audited against the v0.2 spec.
- [ ] Run-level dynamic tool exposure is documented.
- [ ] Tests cover hidden tools, search results and subsequent tool use.
- [ ] The feature can be disabled so all tool schemas are exposed normally.
