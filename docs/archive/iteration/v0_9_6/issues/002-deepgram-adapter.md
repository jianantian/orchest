# 002 · Deepgram adapter

## Background

Deepgram is a strong realtime STT baseline and useful for latency, endpointing and streaming event comparison.

## Goal

Add a Deepgram provider adapter behind a feature flag.

## Acceptance Criteria

- [x] Provider is gated by cargo feature `deepgram`.
- [x] Deepgram provider/model normalization uses `"deepgram/model"`.
- [x] Streaming transcription maps partial/final events into `AsrStreamEvent`.
- [x] `transcribe()` behavior is explicitly implemented or returns `UnsupportedOperation` with tests.
- [x] Capability metadata captures supported languages, timestamps and endpointing behavior.
- [x] Fake tests cover routing and compatibility adjustments.
- [x] Live tests are env-var gated, ignored by default and documented with required variable names.

## Implementation Notes

- The adapter is realtime-only for this issue: `transcribe()` returns `AsrErrorCode::UnsupportedOperation` with model metadata set to `"deepgram/<model>"`.
- `start_stream()` uses the Deepgram WebSocket listen endpoint and maps `Results` messages into provisional/committed transcript updates plus final outputs.
- The initial implemented streaming audio path is PCM16 (`encoding=linear16`) with sample rate and channel query parameters. Additional encoded streaming formats remain future extension work.
- Live test gate:
  - Required: `DEEPGRAM_API_KEY`
  - Optional: `DEEPGRAM_ASR_MODEL` (default `nova-3`)
  - Optional: `DEEPGRAM_ASR_WS_URL` (default `wss://api.deepgram.com/v1/listen`)
