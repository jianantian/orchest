# 003 · Streaming contracts

GitHub: [#119](https://github.com/jianantian/orchest/issues/119)

## Background

TTS streaming has two first-class protocols: single-stream synthesis for complete/committed text and duplex-stream synthesis for incremental text chunks. The public contract must make gateway/direct-provider ownership, terminal events, cancellation and backpressure executable.

## Goal

Implement `TtsOutputStream` and `TtsDuplexStream` behavior so gateway and direct provider streams obey the v0.9.3 lifecycle rules under normal completion, fatal/non-fatal errors, caller cancellation and bounded-channel backpressure.

## Acceptance Criteria

- [ ] Public single-stream API accepts committed text and returns text/audio output events through `TtsOutputStream`
- [ ] Public duplex-stream API lets callers send `TextChunk`s and receive text/audio output events through `TtsDuplexStream`
- [ ] Direct provider streams start with `Started` and never emit `RouteSelected`
- [ ] Gateway streams emit `RouteSelected` before forwarding provider lifecycle events
- [ ] Gateway stream event order is `RouteSelected -> Started -> ... -> Completed/Error`
- [ ] Direct provider stream event order is `Started -> ... -> Completed/Error`
- [ ] `AudioChunk.sequence` is monotonically increasing per stream and starts at `0`
- [ ] `TextDelta.sequence` is monotonically increasing per stream and starts at `0`
- [ ] `Completed` carries `TtsStreamSummary`
- [ ] `Completed` is terminal and no later event is emitted
- [ ] `Error { fatal: true }` is terminal and no later event is emitted
- [ ] `Error { fatal: false }` does not terminate the stream
- [ ] Non-fatal errors are not used for missing audio, auth failures or stream protocol failures
- [ ] Duplex final chunk closes the logical input segment
- [ ] Sending after duplex final chunk fails or produces a fatal caller error
- [ ] Dropping duplex input before final chunk is treated as cancellation when provider has not completed
- [ ] Dropping public event receiver cancels single-stream provider/gateway tasks without indefinite leaks
- [ ] Dropping public event receiver cancels duplex provider/gateway tasks without indefinite leaks
- [ ] `TtsGatewayConfig.stream_channel_capacity` controls public event channel capacity
- [ ] Providers avoid hidden unbounded event channels in tested paths
- [ ] Providers do not require callers to drain provider-internal channels before receiving the public stream handle
- [ ] Streaming tests assert no provider-internal channel drain deadlock
- [ ] Strict and coerce mode still reject unsupported duplex streaming rather than applying a buffered fallback
- [ ] `cargo test -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Notes

Use fake providers and tokio timeouts so cancellation/deadlock failures are visible. Cancellation is best-effort against upstream services, but local gateway/provider tasks must not leak indefinitely. Do not add any implicit "duplex via buffered single-stream" adapter in v0.9.3.

This issue proves the shared streaming contract with fake providers and reusable helpers. Real provider adapters must repeat the transport-specific receiver-drop/session-cleanup/no-hidden-unbounded-channel checks in their own issues.

## Blocked By

- 001 Crate scaffold + public types (#117)
- 002 Gateway + router (#118)
