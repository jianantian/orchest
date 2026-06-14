# 003 · Deferred tool discovery refresh

## Background

Earlier iterations introduced a `search_tools` concept. The current runtime needs a refreshed contract for large tool libraries and model adapter schema injection.

## Goal

Align deferred tool discovery with the current ToolRegistry and run-level tool lifecycle.

## Acceptance Criteria

- [ ] Current `search_tools` behavior is audited against the v0.2 spec.
- [ ] Run-level dynamic tool exposure has a concrete state transition: hidden, returned by `search_tools`, exposed to model schema and callable.
- [ ] Disabling deferred discovery exposes all normal tool schemas directly and omits `search_tools` from the model-visible tool list.
- [ ] Model adapters receive refreshed schemas after a tool is exposed; stale schema behavior is tested.
- [ ] Tests cover hidden tools, search results and subsequent tool use.
- [ ] Tests cover disabled deferred discovery and no-result search behavior.
- [ ] Public examples show large registry discovery and subsequent tool use.
