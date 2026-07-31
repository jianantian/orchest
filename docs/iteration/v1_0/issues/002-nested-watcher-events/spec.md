# 002 · Nested child events for attached watchers

GitHub issue: #250

## Background

SB-8 shows forwarded child events bypass attached watcher subscriptions. SB-4
shows `LlmWatcher` lacks structured formatting for those nested events.

## Goal

Deliver delegated child events to attached watchers and format their
lifecycle/runtime meaning without debug-only fallback text.

## Acceptance Criteria

- [ ] Attached watchers receive the declared delegated child lifecycle and
  runtime events in the supported order.
- [ ] The primary `EventReceiver` remains compatible.
- [ ] `LlmWatcher` produces structured text for each delivered nested event
  family.
- [ ] Tests distinguish parent events, forwarded child events, and duplicate
  delivery.
- [ ] The Research Pipeline verifier closes both SB-4 and SB-8 only after the
  runtime behavior passes.

## Notes

This is a v1.0 pre-freeze gate.
