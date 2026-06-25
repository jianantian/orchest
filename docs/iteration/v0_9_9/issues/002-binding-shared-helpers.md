# 002 · Binding crate shared helpers

## Background

Python and Node bindings duplicate conversion logic, which risks behavioral divergence.

## Goal

Extract FFI-independent shared helpers for duplicated binding conversions without leaking binding concerns into core runtime logic.

## Acceptance Criteria

- [x] Shared helpers cover approval parsing, budget config conversion and event conversion.
- [x] Target-language-only glue remains in the Python/Node crates with a short note when it cannot be shared.
- [x] Business decisions remain in `agent-runtime-core`.
- [x] Python and Node tests continue to pass.
- [x] No unsafe logic is moved into shared helpers.
- [x] Shared helper tests cover equivalent Python/Node conversion behavior.
