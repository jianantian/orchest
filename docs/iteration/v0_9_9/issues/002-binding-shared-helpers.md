# 002 · Binding crate shared helpers

## Background

Python and Node bindings duplicate conversion logic, which risks behavioral divergence.

## Goal

Extract FFI-independent shared helpers where it reduces duplication without leaking binding concerns into core runtime logic.

## Acceptance Criteria

- [ ] Shared helpers cover approval parsing, budget config conversion and event conversion where practical.
- [ ] Business decisions remain in `agent-runtime-core`.
- [ ] Python and Node tests continue to pass.
- [ ] No unsafe logic is moved into shared helpers.
