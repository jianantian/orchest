# 002 · Worker agent and tool set

## Background

The worker Orchest agent is the delegation target. It needs a tool set that is rich enough to exercise the event stream but simple enough not to obscure the delegation mechanics. Fault injection must be first-class.

## Goal

Implement the worker agent with a minimal research tool set and a `fault_trigger` tool. The fake-model path must produce a deterministic event sequence that issues 003 and 004 can attach a watcher to.

## Acceptance Criteria

- [ ] Worker agent is implemented in `src/worker.rs` and accepts a task description via `ContextMode::Fresh` by default.
- [ ] Worker tool set includes at minimum: `search_corpus`, `read_file`, `write_draft`, `fault_trigger`.
- [ ] `fault_trigger` returns `ToolError` with `RetryHint::Unsafe` and `ErrorKind::Fatal` when called, causing the worker run to terminate.
- [ ] Worker emits at least one tool-call event and one model-turn event in the fake-model path.
- [ ] Fake-model smoke test for the worker passes without network access.
- [ ] Worker is not directly runnable as a CLI entry point; it is a library component delegated to by the supervisor.

## Notes

`write_draft` should mark `has_side_effects: true` in tool metadata so the approval path is exercisable if needed. The tool does not need to produce real output in fake-model mode—returning a deterministic string is sufficient.

Tool definitions may be shared with or ported from Briefing Desk if they are exported through a public interface.
