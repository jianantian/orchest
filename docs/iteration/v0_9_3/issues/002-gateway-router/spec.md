# 002 · Gateway + router

GitHub: [#118](https://github.com/jianantian/orchest/issues/118)

## Background

The TTS crate needs a deterministic gateway/router layer so callers can express intent while the SDK selects a provider/model by operation, language, voice kind, format, latency, cost and compatibility policy.

## Goal

Implement `TtsGateway` and `TtsRouter` for batch synthesis, single-stream synthesis, duplex-stream synthesis and voice listing, with request-level provider/model selection and operation-specific compatibility validation.

## Acceptance Criteria

- [ ] `TtsGateway::synthesize()` routes batch synthesis requests to registered fake providers
- [ ] `TtsGateway::stream_synthesize()` routes only to providers with `stream_output == true`
- [ ] `TtsGateway::start_duplex_stream()` routes only to providers with `duplex_streaming == true`
- [ ] `TtsGateway::list_voices()` routes explicit-model requests to the selected provider/model
- [ ] `TtsGateway::list_voices()` supports deterministic route-filtered aggregation when no model is specified
- [ ] Request `model: Some("provider/model")` restricts routing to that normalized provider/model
- [ ] Unprefixed request model strings are rejected
- [ ] Direct provider selector mismatch returns `UnknownModel` or `InvalidRequest`
- [ ] Router filters by operation capability before selecting a provider
- [ ] Router filters batch output formats with `batch_output_formats`
- [ ] Router filters single-stream and duplex-stream output formats with `stream_output_formats`
- [ ] `TtsRoute.output_formats` is treated as route-level preferred/allowed formats, not as a substitute for operation-specific capability validation
- [ ] Router filters by language and explicit request voice-kind route constraints
- [ ] Router applies latency and cost constraints when configured
- [ ] Router selects lowest `priority`, then stable normalized provider/model string sort as tie-breaker
- [ ] Router tie-break behavior is deterministic and tested
- [ ] No matching route returns `NoMatchingProvider` with rejected constraints preserved in metadata where practical
- [ ] Strict compatibility rejects unsupported input kind, operation, language and output format
- [ ] Voice kind is treated as a route/capability constraint when the request already carries an explicit `VoiceSelection.kind`
- [ ] Detailed voice catalog filtering, unresolved voice-kind handling, numeric speech controls and semantic control validation are left to issue 004
- [ ] Strict and coerce compatibility both reject duplex requests for single-stream-only providers
- [ ] Gateway does not create a fake duplex stream by buffering chunks for single-stream synthesis
- [ ] Gateway auto-generates `trace_id` when absent and passes it downstream
- [ ] Fake-provider tests cover routing success, routing failure, voice-list routing/aggregation, operation/format compatibility errors and tie-break behavior
- [ ] `cargo test -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

Router selection order is fixed by the PRD: normalize route model, apply explicit request model restriction, filter by operation capability, validate language/explicit-voice-kind/output constraints, apply latency/cost constraints, then select by priority and stable string sort. `request.compatibility` is the effective runtime policy; gateway config only provides defaults for builders/examples.

This issue intentionally stops at route-level compatibility. Issue 004 owns voice catalog filtering, unresolved voice-kind resolution, speech control numeric ranges and semantic controls such as instruction, style, emotion and SSML.

The acceptance criteria above are the definition of done. The implementation plan must not introduce extra completion requirements that are absent from this spec.

## Blocked By

- 001 Crate scaffold + public types (#117)
