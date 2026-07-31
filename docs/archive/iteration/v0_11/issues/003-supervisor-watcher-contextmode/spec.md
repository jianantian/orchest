# 003 · Supervisor observation and steering attempts

## Background

`AgentAsTool` creates and consumes the delegated worker run internally. The
application owns the supervisor `RunHandle`, not the worker handle. This issue
must exercise that public shape honestly.

## Goal

Implement supervisor delegation, attach watchers to the supervisor, observe
supervisor actor-emitted events on those watcher subscriptions and forwarded
nested-worker events on the primary supervisor `EventReceiver`, and attempt
both watcher and external steering. Demonstrated behavior, the nested-event
routing gap, and blocked child-target behavior are recorded separately.

## Acceptance Criteria

- [x] The supervisor delegates through public `AgentAsTool` /
  `SubAgentBuilder` APIs.
- [x] `LlmWatcher` is built with `.model(...).build()` using its current
  infallible signature.
- [x] A deterministic test gates the supervisor's first model call, attaches
  both watchers through `orchest::run::RunHandle`, awaits both
  `attach_watcher()` calls, releases a harmless probe step, and then proves
  both watcher processors observed the next model step before releasing
  delegation.
- [x] The deterministic assertion proves observable post-registration
  behavior, not capture of every startup event.
- [x] The live path attaches immediately after `AgentRun::start` and records
  that this is best-effort; it does not claim attachment before delegation or
  observation of the first event.
- [x] The missing public start-with-watchers / pre-run pause seam updates its
  pre-seeded finding with source and test evidence.
- [x] Attached watchers receive attributable supervisor actor-emitted events,
  while the primary supervisor `EventReceiver` receives forwarded
  `SubAgentEvent` evidence.
- [x] Terminal-complete event vectors for both the custom watcher and an
  `LlmWatcher` wrapper prove that forwarded child events do not reach attached
  watcher subscription channels; this is recorded as SB-8 rather than
  successful nested watcher observation.
- [x] The implementation attempts to obtain a public delegated-worker target
  and records the absent handle as a gap; it does not use private modules.
- [x] `WatcherAction::Inject` and `WatcherAction::Steer` are exercised against
  the watched supervisor and their actual target is proven. The custom action
  trigger is the supervisor-level
  `ToolCallStarted { tool: "research_worker", .. }`, never a child or nested
  event.
- [x] `RunHandle::inject_message()` and `RunHandle::steer()` are exercised on
  the supervisor handle and their actual target is proven.
- [x] Inability to steer the delegated worker is recorded in the existing
  stable finding rather than represented as success.
- [x] `findings.json` is updated with source and executed evidence for all
  owned checklist entries and findings.

## Public Imports

Use `orchest::run::RunHandle`, `orchest::run::EventReceiver`,
`orchest::run::WatcherAction`, and
`orchest::run::llm_watcher::LlmWatcher`. Private `run::handle` and
`run::config` paths are forbidden.
