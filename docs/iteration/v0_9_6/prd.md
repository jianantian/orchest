# v0.9.6 PRD: ASR Follow-up Providers

## Background

v0.9.1 established the ASR provider gateway with Volcengine and Aliyun and intentionally deferred one-shot transcription plus global provider adapters. Those items are real follow-up work, not loose todo notes, so this iteration turns them into an explicit ASR satellite plan.

## Goals

1. Implement the reserved one-shot `transcribe()` API for complete-audio inputs.
2. Add global realtime ASR provider coverage for interface calibration.
3. Preserve the existing `"provider/model"` selector convention.
4. Keep `agent-runtime-asr-providers` standalone and independent from core runtime.

## Scope

Provider priority:

1. Deepgram realtime STT.
2. ElevenLabs Scribe v2 Realtime.
3. Soniox realtime multilingual/code-switching STT.
4. AssemblyAI transcript metadata and batch shape pressure.
5. Speechmatics multilingual fallback.

The iteration may stage provider implementation behind feature flags, but each provider issue must define fake-provider tests and env-gated live tests.

## Non-Goals

- No core runtime changes.
- No voice-agent session orchestration.
- No audio playback, VAD ownership or WebRTC/SIP integration.
- No ASR-specific variants in core `RuntimeEvent`.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | One-shot transcription contract | File/bytes input behavior, realtime-only fallback, timeout and cancellation |
| 002 | Deepgram adapter | Realtime baseline provider and live-test gate |
| 003 | ElevenLabs Scribe adapter | Realtime Scribe v2 provider and word timestamp/keyterm pressure |
| 004 | Soniox adapter | Multilingual/code-switching provider pressure |
| 005 | AssemblyAI and Speechmatics adapters | Batch/rich metadata and multilingual fallback adapters |
| 006 | ASR provider docs and examples | README/examples for provider selection, `transcribe()` and streaming |

## Acceptance Criteria

- [ ] `transcribe()` has defined behavior for file paths/URLs, byte inputs, realtime-only providers and cancellation.
- [ ] Deepgram, ElevenLabs, Soniox, AssemblyAI and Speechmatics providers exist behind feature flags.
- [ ] Fake-provider tests cover each provider's compatibility and routing behavior.
- [ ] Env-gated live tests are documented for each provider.
- [ ] Public provider/model names follow the existing `"provider/model"` convention.
- [ ] `cargo test -p agent-runtime-asr-providers` passes.
- [ ] `cargo clippy -p agent-runtime-asr-providers -- -D warnings` passes.

## Dependencies

- v0.9.1 ASR Provider Gateway.
- Existing ASR telemetry and routing contracts.
