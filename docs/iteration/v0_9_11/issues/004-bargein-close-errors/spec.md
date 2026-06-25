# Issue 004: Barge-in, close and error semantics

## Background

Realtime omni sessions need clear interruption and shutdown behavior. Some providers support barge-in or cancel semantics; others expose only close/reset behavior. v0.9.11 must implement what the selected provider supports and document limitations precisely.

## Goal / Scope

Implement or document cancel, barge-in, close and error behavior for the experimental realtime provider path.

In scope:

- Graceful session close behavior.
- Provider-supported cancel/barge-in behavior, if available.
- Error mapping for authentication, transport, protocol and provider-side failures.
- Tests for fake-session close/error/cancel behavior.

Out of scope:

- Do not create a universal cancellation model for all providers.
- Do not emulate barge-in if doing so hides provider limitations.
- Do not add retry policy unless required to make the first session stable and documented.

## Acceptance Criteria

- [x] Graceful close is implemented and tested with fake sessions.
- [x] Cancel/barge-in is implemented when supported by the provider or documented as unsupported.
- [x] Authentication, transport, protocol and provider error categories are distinguishable enough for debugging.
- [x] Error handling avoids `unwrap()`/`expect()` in library code.
- [x] Manual-run docs explain how interruption behavior was validated or why it was unavailable.

## Notes

Issue 004 depends on Issue 002 lifecycle scaffolding and may use Issue 003 event mapping for provider error/interruption events. If provider behavior is ambiguous, record the ambiguity as evidence rather than smoothing it over with a generic abstraction.
