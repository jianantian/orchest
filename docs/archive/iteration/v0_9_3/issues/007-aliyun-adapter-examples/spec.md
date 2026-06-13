# 007 · Aliyun adapter + examples

GitHub: [#123](https://github.com/jianantian/orchest/issues/123)

## Background

Aliyun is the second first-party provider target for v0.9.3 and calibrates model-specific differences across CosyVoice realtime and Qwen3-TTS HTTP/realtime/instruct variants. This issue also adds examples after both provider adapters exist.

## Goal

Implement the feature-gated Aliyun adapter, config factory support, offline/live tests and concise examples that show batch, single-stream, duplex-stream and voice-listing usage through the gateway.

## Acceptance Criteria

- [ ] `aliyun` feature compiles
- [ ] Default features include the Aliyun adapter
- [ ] Provider-specific HTTP/WebSocket dependencies are optional unless the implementation PR justifies a shared non-optional dependency
- [ ] `create_tts_provider_from_config()` constructs Aliyun adapter for `aliyun/cosyvoice-v3-flash`
- [ ] `create_tts_provider_from_config()` constructs Aliyun adapter for `aliyun/qwen3-tts-flash-realtime`
- [ ] `create_tts_provider_from_config()` constructs Aliyun adapter for `aliyun/qwen3-tts-instruct-flash-realtime`
- [ ] Unprefixed or non-Aliyun model strings are rejected
- [ ] Direct provider calls with `request.model` set to a different normalized provider/model return `UnknownModel` or `InvalidRequest`
- [ ] Explicit `api_key` is used before env vars
- [ ] Explicit `api_key_env` missing or empty returns `MissingApiKey` and does not fall back
- [ ] Provider default env var is used only when explicit key/env are absent
- [ ] `api_url`, `region`, `timeout` and `provider_options` are preserved in config handling
- [ ] `capabilities()` distinguishes CosyVoice realtime, Qwen3 realtime and instruct model behavior
- [ ] Non-instruct models reject `SpeechControls.instruction` in strict mode
- [ ] Instruct-capable models accept `SpeechControls.instruction` when provider protocol supports it
- [ ] HTTP and realtime model names are not treated as interchangeable
- [ ] Batch and streaming output formats are reported and validated separately
- [ ] Streaming WAV/header caveats are represented through `stream_output_formats` and validation
- [ ] Direct provider streams start with `Started`, not `RouteSelected`
- [ ] Unsupported duplex modes return `UnsupportedOperation` rather than buffered fallback
- [ ] Dropping the public event receiver cancels Aliyun single-stream provider tasks and releases provider sessions/connections without indefinite leaks
- [ ] Dropping the public event receiver cancels Aliyun duplex provider tasks and releases provider sessions/connections without indefinite leaks
- [ ] Aliyun streaming paths avoid hidden unbounded event channels and do not require callers to drain provider-internal channels before receiving the public stream handle
- [ ] `list_voices()` returns `VoiceInfo` with correct provider/model/kind/source metadata
- [ ] Existing custom voice ids can be used for synthesis when provider supports them
- [ ] Voice enrollment/design management APIs are not implemented
- [ ] Provider HTTP/stream errors preserve status/code/message/body with secret redaction
- [ ] Telemetry includes provider/model/operation, first audio latency when available, final latency and output byte count
- [ ] Offline tests cover Aliyun realtime model capability differences
- [ ] Ignored live test synthesizes tiny text and asserts non-empty audio plus telemetry when env vars are set
- [ ] Live tests are gated by explicit Aliyun env vars and ignored by default
- [ ] README/example snippets cover batch, single-stream, duplex-stream and voice-listing gateway usage
- [ ] Examples do not describe TTS as a core Tool or Skill
- [ ] Examples do not commit provider-generated audio samples
- [ ] `cargo test -p agent-runtime-tts-providers --features aliyun` passes without live credentials
- [ ] `cargo test -p agent-runtime-tts-providers --all-features` passes without live credentials
- [ ] `cargo clippy -p agent-runtime-tts-providers --all-features -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

Read `docs/external/aliyun/tts-api-doc.md` and `docs/external/aliyun/tts-guideline.md` before implementation. Aliyun model names encode real capability differences; do not normalize them into one generic adapter behavior. Examples are SDK usage references, not product-level voice session documentation.

This issue must prove Aliyun compliance with shared public contracts, not only compile the adapter. In particular, request selector mismatch, receiver-drop cancellation, session cleanup, model-specific instruction capability, and provider-specific redaction must be covered by offline tests or deterministic fake-transport tests.

## Blocked By

- 001 Crate scaffold + public types (#117)
- 003 Streaming contracts (#119)
- 004 Voice catalog + controls (#120)
- 005 Observability + trace (#121)
- 006 Volcengine adapter (#122)
