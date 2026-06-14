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
- `ContextMode::Fork { depth: usize }`: inherit the latest `depth` parent messages.

Compatibility helpers may remain temporarily, but public docs should prefer `ContextMode`.

### Handoff Transition Safety

Handoff must not leave `AgentRunState` half-mutated if filtering or rebuilding state fails. The preferred implementation is snapshot-then-swap: compute next messages, registry, tool definitions, budget/config decisions first, then replace state in one final section.

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
- No deprecated API removal in v0.9.5; API cleanup is handled by v0.9.9.
- No peer-to-peer agent communication.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Explicit sub-agent ContextMode | Add `ContextMode`, builder API and compatibility path |
| 002 | Handoff transition safety | Replace in-place mutation with snapshot-then-swap or equivalent safe transition |
| 003 | Missing control-flow tests | Add handoff, compaction and supervisor restart coverage |
| 004 | `run_one_step` decomposition | Split the large orchestration function into named phases |
| 005 | Supervised Delegation validation notes | Update docs to keep long-running delegated agent as the validation scenario |

## Acceptance Criteria

- [ ] Public sub-agent builder supports `ContextMode::Fresh` and `ContextMode::Fork { depth }`.
- [ ] Existing `inherit_context_count` usage remains compatible or has an explicit migration note.
- [ ] Fork mode with no available parent context fails loudly or records a clear error; it must not silently behave like Fresh.
- [ ] Handoff state construction completes before mutable run state is replaced.
- [ ] Handoff failure leaves prior run state coherent.
- [ ] Tests cover handoff, compaction and supervisor restart paths.
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
