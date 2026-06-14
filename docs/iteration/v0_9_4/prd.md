# v0.9.4 PRD: Runtime Failure Semantics

## Background

The reviewed backlog identified several small but high-leverage runtime semantics issues: structured tool errors are not expressive enough, `RetryHint` exists but is not consumed by dispatch, and repeated failures do not have a hook point. These issues directly affect product debuggability and will make the v0.10 demo harder to validate if left vague.

v0.9.4 is a focused core-runtime iteration. It does not broaden the agent product surface; it tightens how failures are classified, returned to the model, retried and observed.

## Goals

1. Extend tool error taxonomy with ambiguity and spec-gap semantics.
2. Preserve structured tool error fields when returning failures to the model.
3. Consume `RetryHint` in tool dispatch with bounded, budget-aware retry behavior.
4. Add a minimal repeated-failure hook so applications can intervene after a pattern, not only after isolated errors.
5. Clarify actor handle restart semantics with a small documentation/code-comment cleanup.

## Source Mapping

| Source | Included item |
|--------|---------------|
| External research review | `ErrorKind::Ambiguity` and `ErrorKind::SpecGap` |
| External research review | Tool retry behavior from `RetryHint` and structured error return |
| External research review | `on_repeated_failure` hook |
| Code review backlog | `ActorRef` shared mutex semantics comment |

## Scope

### Error Taxonomy

Add two `ErrorKind` variants:

- `Ambiguity`: the request or specification allows multiple reasonable behaviors. Default next step should guide the model toward clarification, not retry.
- `SpecGap`: the SDK/application contract is missing required behavior. Default next step should guide escalation to the caller or supervising agent.

### Structured Tool Error Return

When tool execution fails, the model-facing tool result must include at least:

- `error.message`
- `error.kind`
- `error.retry`
- `error.code`
- `error.next_step`

The existing human-readable `message` remains available.

### RetryHint Dispatch

Tool dispatch consumes `ToolError.retry`:

- `RetryHint::Safe` with `ErrorKind::Transient`: automatic retry with exponential backoff, max 3 attempts.
- `RetryHint::Unsafe`: no automatic retry.
- `RetryHint::Caution`: request approval before retrying. If approval is denied or unavailable, return the structured error to the model.

Retry attempts must emit events or telemetry sufficient for debugging and must account for existing budget limits.

### Repeated Failure Hook

Add `on_repeated_failure(tool_name, error_history, count)` or an equivalent typed hook context. It triggers when the same tool fails with the same `ErrorKind` for a configurable threshold within one run.

## Non-Goals

- No Draft/Commit tool mode.
- No dynamic `search_tools` / deferred tool registry injection.
- No general policy engine.
- No parallel tool execution.
- No broad `run_one_step` refactor; that belongs to v0.9.5.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | ToolError taxonomy | Add `Ambiguity` / `SpecGap`, constructors, serialization and docs |
| 002 | Structured tool failure return | Preserve `kind`, `retry`, `code`, `next_step` in model-facing tool results |
| 003 | RetryHint dispatch | Implement bounded retry behavior for `Safe`, approval-gated retry for `Caution`, no retry for `Unsafe` |
| 004 | Repeated failure hook | Add repeated-failure tracking and hook invocation |
| 005 | ActorRef restart semantics comment | Clarify why `Arc<Mutex<Option<ActorRef>>>` is rewritten on supervisor restart |

## Acceptance Criteria

- [ ] `ErrorKind` includes `Ambiguity` and `SpecGap` with serde compatibility.
- [ ] Convenience constructors or documented patterns exist for ambiguity and spec-gap errors.
- [ ] Model-facing tool errors include structured fields, not only `message`.
- [ ] Safe transient tool failures retry with bounded exponential backoff.
- [ ] Unsafe tool failures are returned without automatic retry.
- [ ] Caution tool failures ask for approval before retrying.
- [ ] Retry behavior has focused unit or integration tests for all three retry hints.
- [ ] Repeated failure hook triggers only for same tool + same `ErrorKind`.
- [ ] `RunHandle.actor_ref` / supervisor restart comment explains why the mutex is required.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.

## Dependencies

- v0.9 structured `ToolError`.
- Existing approval bus and hook framework.

## Handled By Later Iterations

Draft/Commit and deferred tool discovery remain outside v0.9.4 because they are tool-surface work, not failure-semantics work. They are covered by v0.9.7. Full policy-engine behavior is intentionally represented as app-layer guardrail examples in v0.9.9 rather than a built-in core policy engine.
