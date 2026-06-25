# Issue 004 Plan: Barge-in, close and error semantics

## Files to Read

- `docs/iteration/v0_9_11/prd.md`
- `docs/iteration/v0_9_11/issues/002-duplex-session-scaffolding/spec.md` and lifecycle implementation
- `docs/iteration/v0_9_11/issues/003-streaming-event-mapping/spec.md`
- Selected provider docs for cancel/barge-in/close/error events
- Existing provider error types and streaming shutdown tests

## Files to Change

- Experimental realtime provider session/error modules
- Fake-session tests
- Manual-run documentation or evidence draft

## Steps

1. Identify provider-supported close and interruption messages.
2. Implement graceful close first.
3. Implement cancel/barge-in only if documented and observable.
4. Add provider-specific error mapping for auth, transport, protocol and provider failures.
5. Add fake tests for close, cancel/unsupported-cancel and representative errors.
6. Update manual-run docs with validation instructions.
7. Record unresolved semantics in a form that Issue 005 can copy into the evidence report.
