# 004 · Voice catalog + controls

GitHub: [#120](https://github.com/jianantian/orchest/issues/120)

## Background

Voice identity and speech controls are first-class TTS capabilities. The SDK must distinguish system, cloned, designed and custom voices, validate typed public semantics, and avoid hiding semantic changes inside provider-specific options.

## Goal

Implement voice catalog filtering, voice kind resolution, speech control validation and provider-neutral semantic control handling used by the gateway and provider adapters.

## Acceptance Criteria

- [ ] `VoiceInfo` distinguishes system, cloned, designed and custom voices
- [ ] `VoiceInfo.source` distinguishes provider metadata, static catalog, caller config and conservative assumption
- [ ] Provider voice catalogs can be returned without live credentials when static metadata is available
- [ ] `TtsGateway::list_voices()` filters by language
- [ ] `TtsGateway::list_voices()` filters by `VoiceKind`
- [ ] `TtsGateway::list_voices()` respects `include_custom`
- [ ] Voice list ordering is deterministic
- [ ] Voice filtering builds on the route/aggregation behavior from issue 002 rather than reimplementing provider/model routing
- [ ] Strict mode validates `VoiceSelection.kind` against catalog/capabilities when metadata exists
- [ ] Strict mode does not route unresolved voices to routes that require a specific non-system kind
- [ ] Coerce mode does not silently change requested custom/cloned/designed voice semantics
- [ ] `provider_options` is not the only place where public voice kind semantics are expressed
- [ ] Strict mode rejects unsupported `instruction`, `emotion`, `style` and `SSML`
- [ ] Semantic coercions require `allow_semantic_coercions == true` and record `OptionAdjustment`
- [ ] Strict mode rejects out-of-range speed, pitch and volume with `InvalidRequest`
- [ ] Coerce mode clamps out-of-range speed, pitch and volume and records `OptionAdjustment`
- [ ] Provider-native speech values outside portable ranges flow through `provider_options`, not the portable typed fields
- [ ] Tests cover static catalogs, custom voice inclusion/exclusion, unresolved voice kind, numeric range validation and instruction/SSML support differences
- [ ] `cargo test -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

This issue does not implement voice cloning enrollment, voice design creation, deletion or governance APIs. It may support synthesis with an existing custom voice id when the selected provider supports that behavior.

Issue 002 owns provider/model routing and route-level voice kind constraints when `VoiceSelection.kind` is explicit. This issue owns catalog filtering, unresolved voice-kind resolution, speech control numeric ranges and semantic controls such as instruction, style, emotion and SSML.

The acceptance criteria above are the definition of done. The implementation plan must not introduce extra completion requirements that are absent from this spec.

## Blocked By

- 001 Crate scaffold + public types (#117)
- 002 Gateway + router (#118)
