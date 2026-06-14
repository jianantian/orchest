# 002 · Deepgram adapter

## Background

Deepgram is a strong realtime STT baseline and useful for latency, endpointing and streaming event comparison.

## Goal

Add a Deepgram provider adapter behind a feature flag.

## Acceptance Criteria

- [ ] Provider is gated by cargo feature `deepgram`.
- [ ] Deepgram provider/model normalization uses `"deepgram/model"`.
- [ ] Streaming transcription maps partial/final events into `AsrStreamEvent`.
- [ ] `transcribe()` behavior is explicitly implemented or returns `UnsupportedOperation` with tests.
- [ ] Capability metadata captures supported languages, timestamps and endpointing behavior.
- [ ] Fake tests cover routing and compatibility adjustments.
- [ ] Live tests are env-var gated, ignored by default and documented with required variable names.
