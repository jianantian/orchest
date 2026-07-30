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
- [ ] A deterministic test gates the supervisor's first model call, attaches
  both watchers through `orchest::run::RunHandle`, awaits both
  `attach_watcher()` calls, and only then releases the model to delegate.
- [ ] The deterministic assertion proves observable post-registration
  behavior, not capture of every startup event.
- [ ] The live path attaches immediately after `AgentRun::start` and records
  that this is best-effort; it does not claim attachment before delegation or
  observation of the first event.
- [ ] The missing public start-with-watchers / pre-run pause seam updates its
  pre-seeded finding with source and test evidence.
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
