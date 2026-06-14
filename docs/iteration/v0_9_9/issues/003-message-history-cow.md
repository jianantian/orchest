# 003 · Message history CoW evaluation

## Background

The run loop clones message history per model call and retry. Copy-on-write may reduce allocations, but it changes ownership structure.

## Goal

Evaluate message-history clone cost and implement CoW only if justified.

## Acceptance Criteria

- [ ] A focused benchmark or test captures message-history clone behavior.
- [ ] The benchmark records message count, content size, clone count and allocation impact for model call, retry and handoff paths.
- [ ] If implemented, CoW preserves message ordering and hook semantics.
- [ ] If rejected, the issue notes include profiling evidence and rationale.
- [ ] Any public API change updates examples and tests in the same issue.
