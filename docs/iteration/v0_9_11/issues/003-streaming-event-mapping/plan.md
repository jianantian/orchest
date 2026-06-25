# Issue 003 Plan: Streaming event mapping

## Files to Read

- `docs/iteration/v0_9_11/prd.md`
- `docs/iteration/v0_9_11/issues/001-protocol-fixtures/spec.md` and provider decision output
- `docs/iteration/v0_9_11/issues/002-duplex-session-scaffolding/spec.md` and lifecycle implementation
- `crates/agent-runtime-model/src/` content/event model files
- Existing ASR/TTS streaming event code and tests
- Selected provider protocol docs

## Files to Change

- Experimental realtime provider event mapping module(s) layered on the Issue 002 lifecycle surface
- Fake-session/event tests
- Optional manual-run docs if output examples are added

## Steps

1. Enumerate selected-provider event types from vendor docs and issue 001 notes.
2. Define the minimal internal mapping needed for v0.9.11 observation.
3. Map audio chunks and text/transcript deltas without losing ordering.
4. Handle lifecycle and error events explicitly.
5. Add tool-use handling only if the provider supports it natively; otherwise document unsupported behavior.
6. Add fake interleaving tests.
7. Record any mapping gaps in a form that Issue 005 can copy into the evidence report.
