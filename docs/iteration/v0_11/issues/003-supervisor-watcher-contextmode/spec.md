# 003 · Supervisor observation and steering attempts

## Background

`AgentAsTool` creates and consumes the delegated worker run internally. The
application owns the supervisor `RunHandle`, not the worker handle. This issue
must exercise that public shape honestly.

## Goal

Implement supervisor delegation, attach watchers to the supervisor, observe
forwarded nested-worker events, and attempt both watcher and external
steering. Demonstrated behavior and blocked child-target behavior are recorded
separately.

## Acceptance Criteria

- [ ] The supervisor delegates through public `AgentAsTool` /
  `SubAgentBuilder` APIs.
- [ ] `LlmWatcher` is built with `.model(...).build()` using its current
  infallible signature.
- [ ] Watchers attach to `orchest::run::RunHandle` before delegation.
- [ ] The watcher receives supervisor events and forwarded `SubAgentEvent`
  evidence.
- [ ] The implementation attempts to obtain a public delegated-worker target
  and records the absent handle as a gap; it does not use private modules.
- [ ] `WatcherAction::Inject` and `WatcherAction::Steer` are exercised against
  the watched supervisor and their actual target is proven.
- [ ] `RunHandle::inject_message()` and `RunHandle::steer()` are exercised on
  the supervisor handle and their actual target is proven.
- [ ] Inability to steer the delegated worker is recorded in the existing
  stable finding rather than represented as success.
- [ ] `findings.json` is updated with source and executed evidence for all
  owned checklist entries and findings.

## Public Imports

Use `orchest::run::RunHandle`, `orchest::run::EventReceiver`,
`orchest::run::WatcherAction`, and
`orchest::run::llm_watcher::LlmWatcher`. Private `run::handle` and
`run::config` paths are forbidden.
