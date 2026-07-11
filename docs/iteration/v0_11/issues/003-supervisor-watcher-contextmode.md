# 003 · Supervisor + LlmWatcher + ContextMode

## Background

This issue wires the supervisor to the worker through the Orchest supervised delegation API surface: delegation itself, watcher attachment, and `ContextMode` control. It is the first issue where seam gaps are likely to surface.

## Goal

Implement the supervisor agent, attach an `LlmWatcher` to the worker run, and exercise both `ContextMode::Fresh` and `ContextMode::Fork` paths.

## Acceptance Criteria

- [ ] Supervisor is implemented in `src/supervisor.rs` and delegates to the worker using the public Orchest delegation API.
- [ ] `LlmWatcher` is implemented in `src/watcher.rs` via `LlmWatcher::builder().build()`, attached via `RunHandle::attach_watcher()` before delegation starts, and logs received events to stdout. Import path: `orchest::run::llm_watcher::LlmWatcher`.
- [ ] Watcher detaches cleanly after the worker run completes (implicit on `RuntimeEvent::RunCompleted / RunFailed / RunAborted`).
- [ ] `ContextMode::Fresh` path: `SubAgentBuilder::context_mode(ContextMode::Fresh)` — worker starts with no inherited parent messages. Test asserts that worker context contains no messages from supervisor history. Import path: `orchest::tool::agent_as_tool::ContextMode`.
- [ ] `ContextMode::Fork { depth }` path: `SubAgentBuilder::context_mode(ContextMode::Fork { depth })` — worker inherits the most recent `depth` messages from supervisor history. Test asserts correct count.
- [ ] `ContextMode::Fork` with no inheritable messages produces a clear error rather than silently falling back to `Fresh`.
- [ ] Smoke test for the full supervisor -> worker -> watcher path passes (skipped when RESEARCH_PIPELINE_CHAT_MODEL not set).
- [ ] Any missing or confusing public API entry point encountered during implementation is recorded as a seam gap finding in a `FINDINGS.md` or inline in the validation notes.

## Notes

**Pre-seeded findings to verify**: PSF-1 and PSF-2 from the PRD (LlmWatcher and ContextMode not re-exported from lib.rs). Confirm whether the extra import depth is acceptable ergonomics or warrants a lib.rs re-export before 1.0.

If the public Orchest API for attaching a watcher before delegation start requires reading private source or working around an undocumented constraint, that is a seam gap finding (likely a seam blocker).

Record findings with enough detail to be actionable in issue 005's triage: which API, what the problem is, what the workaround was, and a preliminary classification.
