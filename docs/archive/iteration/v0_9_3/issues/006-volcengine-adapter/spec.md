# 006 · Volcengine adapter

GitHub: [#122](https://github.com/jianantian/orchest/issues/122)

## Background

Volcengine is the first realtime TTS provider target for v0.9.3. Its TTS V3 protocols include bidirectional WebSocket, unidirectional WebSocket, HTTP chunked and SSE modes, and it can support multiple sessions on one connection without concurrent sessions on the same connection.

## Goal

Implement the feature-gated Volcengine adapter with config factory support, capability metadata, voice listing, streaming lifecycle compliance, stable error/telemetry mapping, offline tests and ignored live tests.

## Acceptance Criteria

- [ ] `volcengine` feature compiles
- [ ] Default features include the Volcengine adapter
- [ ] Provider-specific HTTP/WebSocket dependencies are optional unless the implementation PR justifies a shared non-optional dependency
- [ ] `create_tts_provider_from_config()` constructs Volcengine adapter for `volcengine/seed-tts-2.0`
- [ ] Unprefixed or non-Volcengine model strings are rejected
- [ ] Direct provider calls with `request.model` set to a different normalized provider/model return `UnknownModel` or `InvalidRequest`
- [ ] Explicit `api_key` is used before env vars
- [ ] Explicit `api_key_env` missing or empty returns `MissingApiKey` and does not fall back
- [ ] Provider default env var is used only when explicit key/env are absent
- [ ] Old/new credential configuration paths are covered by offline tests
- [ ] `api_url`, `region`, `timeout` and `provider_options` are preserved in config handling
- [ ] `capabilities()` reports batch, stream and duplex support accurately for implemented modes
- [ ] Batch and streaming output formats are reported separately
- [ ] Bidirectional WebSocket and unidirectional streaming modes are not collapsed into one fake protocol
- [ ] Adapter does not expose connection reuse policy as public API
- [ ] Adapter does not run concurrent sessions on the same connection
- [ ] Direct provider streams start with `Started`, not `RouteSelected`
- [ ] Duplex unsupported modes return `UnsupportedOperation` rather than buffered fallback
- [ ] Dropping the public event receiver cancels Volcengine single-stream provider tasks and releases provider sessions/connections without indefinite leaks
- [ ] Dropping the public event receiver cancels Volcengine duplex provider tasks and releases provider sessions/connections without indefinite leaks
- [ ] Volcengine streaming paths avoid hidden unbounded event channels and do not require callers to drain provider-internal channels before receiving the public stream handle
- [ ] `list_voices()` returns `VoiceInfo` with correct provider/model/kind/source metadata
- [ ] Unsupported instruction, SSML, emotion, voice kind and output format map to stable strict-mode errors
- [ ] Provider HTTP/stream errors preserve status/code/message/body with secret redaction
- [ ] Telemetry includes provider/model/operation, first audio latency when available, final latency and output byte count
- [ ] Offline provider tests cover session lifecycle constraints
- [ ] Ignored live test synthesizes tiny text and asserts non-empty audio plus telemetry when env vars are set
- [ ] Live tests are gated by explicit Volcengine env vars and ignored by default
- [ ] `cargo test -p agent-runtime-tts-providers --features volcengine` passes without live credentials
- [ ] `cargo test -p agent-runtime-tts-providers --all-features` passes without live credentials
- [ ] `cargo clippy -p agent-runtime-tts-providers --features volcengine -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

Read `docs/external/volceengine/tts_bidirection.md` before implementation. Bidirectional text input and unidirectional output streaming must remain distinct internally. Provider buffering is allowed only when required by the selected provider protocol and must preserve first-audio latency reporting.

This issue must prove Volcengine compliance with shared public contracts, not only compile the adapter. In particular, request selector mismatch, receiver-drop cancellation, session cleanup, and provider-specific redaction must be covered by offline tests or deterministic fake-transport tests.

## Blocked By

- 001 Crate scaffold + public types (#117)
- 003 Streaming contracts (#119)
- 004 Voice catalog + controls (#120)
- 005 Observability + trace (#121)
