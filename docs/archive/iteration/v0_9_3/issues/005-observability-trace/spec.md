# 005 · Observability + trace

GitHub: [#121](https://github.com/jianantian/orchest/issues/121)

## Background

TTS is an output rendering layer that must correlate with ASR and Orchest run traces without adding TTS-specific core runtime events. Observability must cover batch, single-stream, duplex-stream and voice-listing operations.

## Goal

Implement trace propagation, telemetry summaries, spans, metrics and redaction for gateway and provider operations.

## Acceptance Criteria

- [ ] Batch `SynthesizeResult.telemetry` includes trace id, provider/model, optional voice id and `TtsOperation::Batch`
- [ ] Single-stream `Completed` summary includes `TtsOperation::SingleStream`
- [ ] Duplex-stream `Completed` summary includes `TtsOperation::DuplexStream`
- [ ] Voice listing telemetry uses `TtsOperation::ListVoices` and `voice_id: None` when emitted
- [ ] Gateway-generated trace id is stable across all events in a stream
- [ ] Caller-provided trace id is preserved across all events in a stream
- [ ] `RouteSelected`, `Started`, `TextDelta`, `TextAccepted`, `AudioChunk`, `Completed` and `Error` include the same trace id for one request
- [ ] Provider adapters do not generate unrelated trace ids once gateway has supplied one
- [ ] First-audio latency is recorded when an audio chunk is emitted
- [ ] Final latency is recorded for successful and failed provider operations
- [ ] Input chars are recorded for batch and streaming operations
- [ ] Output bytes are recorded when available
- [ ] Option adjustment count matches recorded adjustments
- [ ] Provider HTTP/status codes are preserved in telemetry/errors when available
- [ ] Error redaction removes API keys and auth headers from upstream bodies/metadata
- [ ] Raw audio bytes are not logged by default
- [ ] Full synthesized text is not logged by default
- [ ] Temporary provider URLs are not logged by default
- [ ] Spans exist for `tts.gateway.synthesize`, `tts.gateway.stream`, `tts.provider.request`, `tts.provider.stream`, `tts.router.select` and `tts.voices.list`
- [ ] Metrics exist for request duration, first audio latency, final synthesis latency, input chars, output bytes, audio duration, provider error count and option adjustment count
- [ ] Metrics use low-cardinality labels only
- [ ] No TTS-specific variants are added to core `RuntimeEvent`
- [ ] `agent-runtime-core` is not modified
- [ ] `cargo test -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

Read `docs/polaris/observability.md` before implementation. Text/audio logging remains application policy; the SDK default should preserve correlation without logging sensitive payloads.

This issue owns shared observability helpers and fake-provider coverage. Real provider adapters must still prove provider-specific upstream status/body preservation and redaction in their own issues.

## Blocked By

- 001 Crate scaffold + public types (#117)
- 002 Gateway + router (#118)
- 003 Streaming contracts (#119)
