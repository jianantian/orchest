# v0.9.9 PRD: API Cleanup and Product Patterns

## Background

The remaining code-review and research items are real work, but they are best handled as API cleanup and product-pattern deliverables rather than vague backlog. This iteration closes those items before v0.10 validation.

## Goals

1. Remove deprecated APIs in the pre-v1.0 breaking-change window.
2. Reduce binding crate divergence.
3. Evaluate and implement message-history copy-on-write only if tests/profiling justify it.
4. Provide product-layer examples for complex guardrails, agent teams and direct coordination patterns.

## Non-Goals

- No full built-in permission policy engine in core.
- No mandatory peer-to-peer agent protocol unless the examples prove a minimal runtime primitive is required.
- No UI or hosted team orchestration product.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Deprecated API removal | Remove `as_tool_legacy`, `SideEffectOnly`, `AgentAsTool::new` and binding aliases |
| 002 | Binding crate shared helpers | Extract duplicated FFI-independent conversion helpers |
| 003 | Message history CoW evaluation | Profile/test clone cost and implement CoW only if justified |
| 004 | Guardrail policy examples | Show complex policies as app-layer guardrails |
| 005 | Agent team pattern examples | Provide examples/templates for team coordination without bloating core |

## Acceptance Criteria

- [x] Deprecated APIs listed in issue 001 are removed from Rust, Python and Node surfaces.
- [x] Deprecated API removal has migration notes and updated examples/tests.
- [x] Binding helper extraction does not change Python/Node behavior.
- [x] Message CoW is either implemented with tests or rejected with profiling evidence in the issue notes.
- [x] Complex permission policy remains app-layer unless a core gap is proven.
- [x] Team patterns are available as examples or docs.
- [x] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.
