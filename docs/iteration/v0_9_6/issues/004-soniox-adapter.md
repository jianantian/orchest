# 004 · Soniox adapter

## Background

Soniox provides useful pressure for multilingual and code-switching behavior.

## Goal

Add a Soniox provider adapter behind a feature flag.

## Acceptance Criteria

- [x] Provider is gated by cargo feature `soniox`.
- [x] Capability metadata represents multilingual and code-switching support.
- [x] `Language` and routing behavior handle mixed-language requests without hard-coded closed sets.
- [x] Streaming partial/final events normalize to the common ASR stream model.
- [x] `transcribe()` behavior is explicitly implemented or returns `UnsupportedOperation` with tests.
- [x] Fake and env-gated live tests cover code-switching options and document required variable names.

## Implementation Notes

- The implemented adapter is realtime-only for `soniox/stt-rt-v5` and `soniox/stt-rt-v4`; `transcribe()` returns `AsrErrorCode::UnsupportedOperation` with model metadata set to `"soniox/<model>"`.
- `TranscribeOptions.code_switching`, arbitrary `Language` tags such as `"mixed:en,es"`, `provider_options.language_hints`, and `provider_options.enable_language_identification` map into the Soniox realtime config message.
- Streaming Soniox `tokens` map final tokens to committed `TranscriptUpdate` events, non-final tokens to provisional snapshot updates, and final/caller boundaries to `AsrFinal`.
- Live test gate:
  - Required: `SONIOX_API_KEY`
  - Optional: `SONIOX_ASR_MODEL` (default `stt-rt-v5`)
  - Optional: `SONIOX_ASR_WS_URL` (default `wss://stt-rt.soniox.com/transcribe-websocket`)
