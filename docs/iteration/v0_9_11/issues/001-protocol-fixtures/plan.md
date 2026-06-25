# Issue 001 Plan: Protocol decision, vendor notes and fixtures

## Files to Read

- `docs/iteration/v0_9_11/prd.md`
- `docs/todo/provider-unification.md`
- `docs/external/volceengine/realtime.md`
- Existing ASR/TTS provider docs and tests that describe duplex fixture shape

## Files to Change

- Fixture files or fixture-contract docs under the selected provider/example test location
- A provider decision note under `docs/iteration/v0_9_11/`

## Steps

1. Re-read the PRD and confirm the issue breakdown is still correct.
2. Read the preferred provider documentation and record endpoint/auth/session/event assumptions.
3. Decide whether Volcengine realtime remains the first target or whether Qwen omni replaces it with newly added vendor docs.
4. Define the minimal audio/text fixture contract for fake-session tests, including path, format and sample-rate expectations.
5. Record whether a second provider is accepted or deferred.
6. Verify this issue only changes docs/fixtures and does not introduce provider networking implementation code.
