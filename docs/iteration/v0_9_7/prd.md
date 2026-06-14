# v0.9.7 PRD: Tool Surface Extensions

## Background

The external research backlog identified several tool-surface capabilities that are real product work: safer high-risk actions, deferred tool discovery and optional parallel tool execution. This iteration gives them a concrete runtime plan instead of leaving them as loose ideas.

## Goals

1. Add a standard Draft/Commit pattern for high-risk tools.
2. Revisit deferred tool discovery against the current ToolRegistry and model adapter behavior.
3. Add optional parallel tool execution with explicit safety constraints.

## Scope

### Draft/Commit

Extend `ToolMetadata` with `ToolExecutionMode::Normal | Draft { commit_tool } | Commit { draft_tool }`. Draft tools must be side-effect-free and approval-free by default. Commit tools must require approval by default, even when run-level approval mode is permissive, unless the application deliberately overrides approval through a documented custom approval function.

Tool registration must validate that draft/commit links reference registered tools, do not point at themselves and do not form ambiguous many-to-one relationships.

### Deferred Discovery

Refresh `search_tools` against the current `ToolRegistry` and model adapter schema injection path. Hidden tools discovered through `search_tools` must become callable in the same run only through a documented exposure step; disabling deferred discovery exposes all normal tool schemas directly and removes `search_tools` from the model-visible tool list.

### Parallel Execution

Add a runtime config field for tool parallelism, disabled by default. Extend tool metadata with `ToolParallelism::Serial | ParallelSafe`. Only tools marked parallel-safe and not awaiting approval may execute concurrently. Event metadata must include batch id and per-call sequence fields sufficient to reconstruct the model-requested order and completion order.

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
| 005 | Tool surface docs and examples | Document usage patterns and examples |

## Acceptance Criteria

- [ ] High-risk tools can expose a preview path and a commit path.
- [ ] Draft calls produce no side effects and require no approval by default.
- [ ] Commit calls require approval by default.
- [ ] Deferred tool discovery behavior is documented and tested against current tool schema injection.
- [ ] Parallel execution is opt-in and respects approval, budget, timeout and event ordering constraints.
- [ ] `ToolMetadata` serialization and Rust/Python/Node bindings cover Draft/Commit and parallel-safety metadata.
- [ ] Public examples and tests are updated for Draft/Commit, `search_tools` and parallel execution.
- [ ] Existing sequential behavior remains the default.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.
