# 002 · Deepgram adapter

## Background

Deepgram is a strong realtime STT baseline and useful for latency, endpointing and streaming event comparison.

## Goal

Add a Deepgram provider adapter behind a feature flag.

## Acceptance Criteria

- [ ] Deepgram provider/model normalization uses `"deepgram/model"`.
- [ ] Streaming transcription maps partial/final events into `AsrStreamEvent`.
- [ ] Capability metadata captures supported languages, timestamps and endpointing behavior.
- [ ] Fake tests cover routing and compatibility adjustments.
- [ ] Live tests are env-var gated and ignored by default.
