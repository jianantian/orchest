# 003 · ElevenLabs Scribe adapter

## Background

ElevenLabs Scribe v2 Realtime is useful interface pressure for realtime WebSocket STT, word timestamps, keyterm prompting and diarization-related semantics.

## Goal

Add an ElevenLabs Scribe provider adapter behind a feature flag.

## Acceptance Criteria

- [x] Provider is gated by cargo feature `elevenlabs`.
- [x] Model examples include `elevenlabs/scribe_v2_realtime` and `elevenlabs/scribe_v2` where applicable.
- [x] Keyterm prompting and language detection map to typed config or `provider_options`.
- [x] Word-level timestamps are represented without changing core runtime.
- [x] Unsupported diarization semantics are explicit, not silently dropped.
- [x] `transcribe()` behavior is explicitly implemented or returns `UnsupportedOperation` with tests.
- [x] Fake and env-gated live tests are documented with required variable names.

## Implementation Notes

- The implemented adapter is realtime-only for `elevenlabs/scribe_v2_realtime`; `transcribe()` returns `AsrErrorCode::UnsupportedOperation` with model metadata set to `"elevenlabs/<model>"`.
- `elevenlabs/scribe_v2` is listed in the static catalog as the related batch model, but the batch HTTP adapter is out of scope for this realtime issue.
- `TranscribeOptions.hot_words` and `provider_options.keyterms` map to ElevenLabs `keyterms`; `provider_options.include_language_detection` controls language detection and defaults on when no explicit language is provided.
- Timestamped committed transcripts map to existing `WordTimestamp`, so no core runtime type changes are needed.
- Diarization remains unsupported: capabilities advertise `speaker_diarization = false`, compatibility tests cover adjustment/rejection, and direct provider calls reject it before network connection.
- Live test gate:
  - Required: `ELEVENLABS_API_KEY`
  - Optional: `ELEVENLABS_ASR_MODEL` (default `scribe_v2_realtime`)
  - Optional: `ELEVENLABS_ASR_WS_URL` (default `wss://api.elevenlabs.io/v1/speech-to-text/stream`)
