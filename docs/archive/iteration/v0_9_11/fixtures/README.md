# v0.9.11 Realtime Fixtures

These fixtures support Issue 001 and later fake-session tests. They are intentionally tiny and deterministic; live provider validation remains credential-gated.

## Files

| File | Purpose |
|------|---------|
| `silence_20ms_16k_s16le.pcm` | One 20 ms mono 16 kHz signed 16-bit little-endian PCM silence chunk, exactly 640 bytes |
| `start_session_audio_file_o2.json` | Minimal StartSession payload for deterministic `audio_file` input and PCM TTS output |
| `text_query.json` | Text-only query fixture for validating non-audio input plumbing when needed |
| `fake_event_sequence.json` | Ordered fake server events for lifecycle, transcript, text and audio-output mapping tests |

## Audio Contract

- Input PCM: mono, 16 kHz, signed 16-bit little-endian.
- Deterministic chunk size: 20 ms = 640 bytes.
- The silence fixture contains only zeros and is not a meaningful speech sample. It exists to validate chunk framing, fixture loading and fake-session sequencing.
- Real provider validation should use either microphone streaming or a real recording converted to the same PCM format.

## Manual Validation Notes

Use the environment variables recorded in `../provider-decision.md`. Do not commit live credentials or real user audio.
