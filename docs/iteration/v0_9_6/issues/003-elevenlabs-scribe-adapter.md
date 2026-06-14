# 003 · ElevenLabs Scribe adapter

## Background

ElevenLabs Scribe v2 Realtime is useful interface pressure for realtime WebSocket STT, word timestamps, keyterm prompting and diarization-related semantics.

## Goal

Add an ElevenLabs Scribe provider adapter behind a feature flag.

## Acceptance Criteria

- [ ] Provider is gated by cargo feature `elevenlabs`.
- [ ] Model examples include `elevenlabs/scribe_v2_realtime` and `elevenlabs/scribe_v2` where applicable.
- [ ] Keyterm prompting and language detection map to typed config or `provider_options`.
- [ ] Word-level timestamps are represented without changing core runtime.
- [ ] Unsupported diarization semantics are explicit, not silently dropped.
- [ ] `transcribe()` behavior is explicitly implemented or returns `UnsupportedOperation` with tests.
- [ ] Fake and env-gated live tests are documented with required variable names.
