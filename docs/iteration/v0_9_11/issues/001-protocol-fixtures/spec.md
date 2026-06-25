# Issue 001: Protocol decision, vendor notes and fixtures

## Background

v0.9.11 starts from `docs/todo/provider-unification.md` Step 2: collect real omni/realtime provider evidence before designing provider unification. Before provider implementation begins, the iteration needs a concrete provider decision, vendor protocol notes and deterministic fixtures so later issues do not invent scope ad hoc.

The preferred first target is Doubao / Volcengine realtime because `docs/external/volceengine/realtime.md` already exists. Qwen omni may replace it only if official vendor documentation is added under `docs/external/` and the decision is recorded.

## Goal / Scope

Create the protocol decision record and test fixture contract required to start provider implementation safely.

In scope:

- Decide the first realtime/omni provider target.
- Record endpoint/auth/session/event assumptions from official or checked-in vendor docs.
- Define or add small audio/text fixtures suitable for fake-session tests and manual-run examples.
- Decide whether a second provider is accepted or explicitly deferred.

Out of scope:

- Do not implement provider networking in this issue.
- Do not define the final provider-unification abstraction.
- Do not modify existing ASR/TTS/LLM/AIGC public APIs.

## Acceptance Criteria

- [x] The selected first provider is recorded with vendor source references in `docs/iteration/v0_9_11/provider-decision.md`.
- [x] Required credentials, endpoint settings and model/session identifiers are documented.
- [x] Input fixture expectations are documented, including audio format, sample rate, storage path and text metadata if applicable.
- [x] A second-provider decision is recorded as accepted or deferred.
- [x] No runtime/provider networking implementation code is introduced by this issue.

## Notes

This issue is the provider-selection gate for the rest of v0.9.11. If protocol documentation contradicts the PRD, update the PRD before implementing later issues.
