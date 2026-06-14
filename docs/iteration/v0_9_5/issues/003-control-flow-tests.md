# 003 · Missing control-flow tests

## Background

The code review backlog identified missing end-to-end tests for handoff, compaction and supervisor crash/restart. These paths are core to the v0.10 demo and must be covered before deeper refactors.

## Goal

Add focused tests for handoff transition, compaction summary injection and supervisor restart state replay.

## Acceptance Criteria

- [ ] A test exercises a model response that triggers `ToolOutput::Handoff`.
- [ ] The handoff test asserts target agent config, message history and emitted events.
- [ ] A compaction test triggers the configured threshold and asserts summary injection.
- [ ] A supervisor restart test verifies state replay after worker crash.
- [ ] Tests use local fakes, not network providers.
- [ ] `cargo test -p agent-runtime-core handoff` and related focused test commands pass.

## Notes

Write these tests before the `run_one_step` decomposition so the refactor has safety rails.
