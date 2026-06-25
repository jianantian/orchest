# 005 · Agent team pattern examples

## Background

Research suggested direct agent communication and team templates. These should first be validated as product-layer patterns.

## Goal

Provide examples/templates for agent team coordination using existing primitives.

## Acceptance Criteria

- [x] Examples cover at least two team patterns using Agent-as-Tool, Handoff or watchers.
- [x] Docs identify when existing primitives are sufficient.
- [x] Examples use `ContextMode` rather than removed `inherit_context(...)` helpers.
- [x] If direct peer-to-peer communication is required, a follow-up runtime issue is created with evidence from a failing or awkward example.
- [x] Examples compile and run in CI.

## Notes

Added `docs/guide/agent-team-patterns.md`, which documents three product-layer
team patterns:

- parent coordinator with specialist Agent-as-Tool;
- triage-to-specialist Handoff;
- external supervisor Watcher.

The referenced examples are `agent_as_tool`, `handoff_routing`,
`handoff_input_filter`, `watcher_inject_message`, `watcher_abort_on_pattern`,
and `supervised_delegation`. Agent-as-Tool examples use `ContextMode` directly;
`inherit_context(...)` does not exist in the current examples.

No direct peer-to-peer runtime issue was created. The examples did not prove a
core gap: bounded delegation, ownership transfer, and external supervision are
covered by existing primitives.
