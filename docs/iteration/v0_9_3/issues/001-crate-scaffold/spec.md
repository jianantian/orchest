# 001 · Crate scaffold + public types

GitHub: [#117](https://github.com/jianantian/orchest/issues/117)

## Background

v0.9.3 introduces `agent-runtime-tts-providers`, a standalone satellite crate for text-to-speech provider integration. This issue establishes the crate, module boundaries, public trait, public data model, error model, configuration shape and provider stubs that every later issue depends on.

## Goal

Create the TTS provider crate with zero workspace-internal dependencies and define the provider-neutral public API for batch synthesis, single-stream synthesis, duplex-stream synthesis and voice listing.

## Acceptance Criteria

- [ ] `crates/agent-runtime-tts-providers` exists and is part of the workspace
- [ ] Crate has zero workspace-internal dependencies
- [ ] `agent-runtime-core` is not modified
- [ ] `anyhow` is not used in the library crate
- [ ] Modules exist for `lib`, `traits`, `types`, `streaming`, `voices`, `routing`, `observability`, `error`, `config` and `providers`
- [ ] `TtsProvider` trait supports batch synthesis, single-stream synthesis, duplex streaming synthesis and voice listing
- [ ] Provider-neutral request/result types exist for `SynthesizeRequest`, `StreamSynthesizeRequest`, `DuplexSynthesizeRequest`, `SynthesizeResult` and `TtsStreamSummary`
- [ ] Provider-neutral voice types exist for `VoiceSelection`, `VoiceInfo`, `VoiceKind`, `VoiceGender` and `ListVoicesRequest`
- [ ] Provider-neutral speech controls exist, with documented portable ranges: speed `0.5..=2.0`, pitch `-12.0..=12.0`, volume `0.0..=2.0`
- [ ] Provider-neutral audio types distinguish `Pcm16Le`, `WavPcm16Le`, `Mp3` and `OggOpus`
- [ ] Stream types exist for `TtsOutputStream`, `TtsDuplexStream`, `TextChunk` and `TtsStreamEvent`
- [ ] Capability types exist, including separate `batch_output_formats` and `stream_output_formats`
- [ ] Routing/config types exist for `TtsGateway`, `TtsGatewayConfig`, `TtsRouter`, `TtsRoute`, `TtsProviderRuntimeConfig` and `NormalizedTtsProviderModel`
- [ ] `TtsTelemetry` includes `operation: TtsOperation` and `voice_id: Option<String>`
- [ ] `TtsError` and `TtsErrorCode` cover the PRD error variants, including `Cancelled`
- [ ] `normalize_tts_provider_model()` rejects bare model strings without provider prefix
- [ ] Provider feature flags `volcengine`, `aliyun` and default feature membership are defined
- [ ] Provider modules compile as stubs behind their feature flags
- [ ] `cargo check -p agent-runtime-tts-providers` passes
- [ ] `cargo check -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

Read the existing provider crates before implementing: `agent-runtime-providers`, `agent-runtime-aigc-providers` and `agent-runtime-asr-providers`. `Language` should remain a string newtype because BCP-47 is an open set. `TtsStreamSummary` should stay separate from `SynthesizeResult` so streaming completion does not imply full audio bytes are retained in memory.

## Blocked By

None.
