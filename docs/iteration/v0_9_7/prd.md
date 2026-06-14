# v0.9.7 PRD: Tool Surface Extensions

## Background

The external research backlog identified several tool-surface capabilities that are real product work: safer high-risk actions, deferred tool discovery and optional parallel tool execution. This iteration gives them a concrete runtime plan instead of leaving them as loose ideas.

## Goals

1. Add a standard Draft/Commit pattern for high-risk tools.
2. Revisit deferred tool discovery against the current ToolRegistry and model adapter behavior.
3. Add optional parallel tool execution with explicit safety constraints.

## Non-Goals

- No full policy engine.
- No agent peer-to-peer protocol.
- No product-specific tool catalog.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Draft/Commit metadata | Tool metadata shape and dispatch semantics |
| 002 | Draft/Commit approval behavior | Draft runs without side effects; commit requires approval |
| 003 | Deferred tool discovery refresh | Align `search_tools` behavior with current registry/model contracts |
| 004 | Optional parallel tool execution | Execute independent tool calls concurrently when safe |
| 005 | Tool surface docs and examples | Document patterns and migration guidance |

## Acceptance Criteria

- [ ] High-risk tools can expose a preview path and a commit path.
- [ ] Draft calls produce no side effects and require no approval by default.
- [ ] Commit calls require approval by default.
- [ ] Deferred tool discovery behavior is documented and tested against current tool schema injection.
- [ ] Parallel execution is opt-in and respects approval, budget, timeout and event ordering constraints.
- [ ] Existing sequential behavior remains the default.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.
