# Issue 002: Duplex session scaffolding

## Background

After the provider protocol is selected, v0.9.11 needs a narrow realtime session path that can start, accept input, expose output hooks for later event mapping and close. This scaffolding is intentionally experimental and must not become the final provider-unification abstraction.

## Goal / Scope

Add feature-gated client/session lifecycle scaffolding for the selected realtime provider, plus fake-session tests that run without credentials.

In scope:

- Provider config and credential loading for the selected realtime target.
- Session start/update/close lifecycle primitives.
- Input path for audio chunks and optional text metadata.
- Deterministic fake-session tests for lifecycle behavior.
- Feature gating for new networking, websocket, signing or audio transport dependencies.

Out of scope:

- Do not stabilize a public provider-core abstraction.
- Do not merge provider crates.
- Do not require live credentials in CI.
- Do not implement final event mapping beyond what is needed to prove lifecycle wiring.

## Acceptance Criteria

- [x] The selected provider path is guarded by an appropriate feature flag if it adds optional heavy dependencies.
- [x] Config loading documents every required environment variable.
- [x] A fake session can start, accept at least one audio input chunk and close deterministically.
- [x] Network/live provider tests are ignored or manual-only unless credentials are explicitly configured.
- [x] Lifecycle errors are represented with provider-appropriate error types and do not use `unwrap()`/`expect()` in library code.
- [x] Existing ASR/TTS/LLM/AIGC provider APIs continue to compile unchanged.

## Notes

Issue 002 depends on Issue 001 provider-selection artifacts. Keep the surface minimal. If this issue needs a reusable helper that looks like provider-core, keep it private/internal and record the pressure in the later evidence report instead of stabilizing it now.
