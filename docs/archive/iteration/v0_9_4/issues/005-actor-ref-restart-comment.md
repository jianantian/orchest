# 005 · ActorRef restart semantics comment

## Background

The code review backlog notes that `Arc<Mutex<Option<ActorRef>>>` looks unnecessary if the actor reference is set once, but supervisor restart rewrites it. The code should say that directly.

## Goal

Clarify the concurrency/restart invariant in code comments.

## Acceptance Criteria

- [x] `crates/agent-runtime-core/src/run/handle.rs` documents that `actor_ref` is rewritten after supervisor restarts.
- [x] The comment mentions concurrent watcher access as the reason for the mutex.
- [x] No behavior changes are made.
- [x] `cargo fmt --check` passes.

## Notes

This is intentionally tiny and should not turn into a handle refactor.
