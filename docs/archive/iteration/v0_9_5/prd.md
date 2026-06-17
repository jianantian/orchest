# v0.9.5 PRD: Agent Control-Flow Hardening

## Background

The v0.10 demo will exercise session resume, reviewer sub-agents or handoff, event streaming and approval. The code review backlog also identifies that `run_one_step` mixes too many control-flow stages and that handoff mutates state in place. These are not v1.0 release mechanics; they are runtime hardening work that should happen before the demo relies on these paths.

v0.9.5 is a focused hardening iteration for sub-agent context semantics, handoff transition safety, run-loop decomposition and missing end-to-end tests.

## Goals

1. Make sub-agent context inheritance explicit with `ContextMode`.
2. Make handoff state transitions atomic or otherwise rollback-safe.
3. Split `run_one_step` into named phases without changing behavior.
4. Add missing handoff, compaction and supervisor restart tests.
5. Re-anchor Supervised Delegation as a validation case for the demo and future long-running agent tools.

## Source Mapping

| Source | Included item |
|--------|---------------|
| External research review | `ContextMode::Fresh | Fork { depth }` |
| Code review backlog | `run_one_step` decomposition |
| Code review backlog | Handoff transition safety |
| Code review backlog | Handoff / compaction / crash-recovery tests |
| Product validation notes | Supervised Delegation remains the validation case |

## Scope

### ContextMode

Replace implicit `inherit_context_count: Option<usize>` semantics with explicit context mode:

- `ContextMode::Fresh`: no parent history.
- `ContextMode::Fork { depth }`: inherit the latest `depth` parent messages.

This is a breaking cleanup. Remove the `inherit_context_count` field and the `inherit_context(...)` builder helper instead of carrying compatibility wrappers. Use a non-zero depth type if practical, or reject `depth == 0` during builder validation. Public examples and tests must use `context_mode(ContextMode::...)`.

### Handoff Transition Safety

Handoff must not leave `AgentRunState` half-mutated if filtering or rebuilding state fails. The preferred implementation is snapshot-then-swap: compute next messages, registry, tool definitions, budget/config decisions first, then replace state in one final section.

Make `HandoffInputFilter::filter` fallible, for example `Result<HandoffInputData, HandoffError>` or the local equivalent. A filter failure must emit a structured failure and leave the previous run state coherent.

### Run Loop Decomposition

Split `run_one_step` into named internal phases such as:

- limit and budget checks
- model call with before/after hooks
- tool call execution
- handoff processing
- message/state finalization

Do not change public API in this refactor.

### Missing Tests

Add focused tests for:

- Handoff state transition and target-agent config.
- Compaction trigger and summary injection.
- Supervisor crash-and-restart state replay.

## Non-Goals

- No message-history zero-copy rewrite; `Arc<[Message]>` remains deferred until evidence shows it matters.
- No binding crate deduplication.
- No code execution sandbox injection.
- No broad deprecated API sweep in v0.9.5; the old context inheritance API is removed here because it is part of this iteration's core contract.
- No peer-to-peer agent communication.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Explicit sub-agent ContextMode | Add `ContextMode` and replace the old inheritance API |
| 002 | Handoff transition safety | Replace in-place mutation with snapshot-then-swap or equivalent safe transition |
| 003 | Missing control-flow tests | Add handoff, compaction and supervisor restart coverage |
| 004 | `run_one_step` decomposition | Split the large orchestration function into named phases |
| 005 | Supervised Delegation validation notes | Update docs to keep long-running delegated agent as the validation scenario |

## Acceptance Criteria

- [ ] Public sub-agent builder supports `ContextMode::Fresh` and `ContextMode::Fork { depth }`.
- [ ] Old `inherit_context_count` storage and `inherit_context(...)` builder helper are removed.
- [ ] Fork depth is non-zero by type or rejected during builder validation.
- [ ] Fork mode with no available parent context fails loudly or records a clear error; it must not silently behave like Fresh.
- [ ] Handoff state construction completes before mutable run state is replaced.
- [ ] Handoff input filters are fallible, and filter failure leaves prior run state coherent.
- [ ] Handoff failure leaves prior run state coherent.
- [ ] Tests cover handoff, compaction and supervisor restart paths.
- [ ] Public examples and tests are updated for `ContextMode` and fallible handoff filters.
- [ ] `run_one_step` no longer requires `#[allow(clippy::too_many_lines)]`.
- [ ] Refactor does not change externally visible runtime events except where explicitly documented.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` pass.

## Dependencies

- v0.9.4 structured failure semantics are recommended first so new tests can assert richer errors.
- Existing v0.8 session/supervisor support.
- Existing v0.9 Agent-as-Tool, Handoff and Steering APIs.

## Handled By Later Iterations

The following items stay outside v0.9.5 because this iteration is limited to agent control-flow hardening:

- ASR one-shot transcription and additional ASR providers: v0.9.6.
- Draft/Commit tool mode and deferred tool discovery / `search_tools`: v0.9.7.
- Code execution sandbox injection and expanded observability metrics: v0.9.8.
- Message-history zero-copy evaluation, binding crate deduplication and deprecated API removal: v0.9.9.
