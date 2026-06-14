# 003 · Message history CoW evaluation

## Background

The run loop clones message history per model call and retry. Copy-on-write may reduce allocations, but it changes ownership structure.

## Goal

Evaluate message-history clone cost and implement CoW only if justified.

## Acceptance Criteria

- [ ] A focused benchmark or test captures message-history clone behavior.
- [ ] If implemented, CoW preserves message ordering and hook semantics.
- [ ] If rejected, the issue notes include profiling evidence and rationale.
- [ ] No public API changes are introduced without migration notes.
