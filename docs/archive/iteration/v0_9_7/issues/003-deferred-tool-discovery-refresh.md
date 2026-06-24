# 003 · Deferred tool discovery refresh

## Background

Earlier iterations introduced a `search_tools` concept. The current runtime needs a refreshed contract for large tool libraries and model adapter schema injection.

## Goal

Align deferred tool discovery with the current ToolRegistry and run-level tool lifecycle.

## Acceptance Criteria

- [x] Current `search_tools` behavior is audited against the v0.2 spec.
- [x] Run-level dynamic tool exposure has a concrete state transition: hidden, returned by `search_tools`, exposed to model schema and callable.
- [x] Disabling deferred discovery exposes all normal tool schemas directly and omits `search_tools` from the model-visible tool list.
- [x] Model adapters receive refreshed schemas after a tool is exposed; stale schema behavior is tested.
- [x] Tests cover hidden tools, search results and subsequent tool use.
- [x] Tests cover disabled deferred discovery and no-result search behavior.
- [x] Public examples show large registry discovery and subsequent tool use.

## Notes

Audit against `docs/archive/iteration/v0_2/issues/003-tool-search-tool/spec.md`:
the current runtime already exposed only `search_tools` initially and appended
returned schemas to later model calls. The refresh closes two gaps: hidden tools
are no longer callable by guessed name before exposure, and zero-score searches
return no results instead of arbitrarily exposing unrelated tools.
