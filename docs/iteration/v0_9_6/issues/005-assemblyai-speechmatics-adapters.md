# 005 · AssemblyAI and Speechmatics adapters

## Background

AssemblyAI pressures rich transcript metadata and batch result shape; Speechmatics pressures multilingual enterprise fallback behavior.

## Goal

Add AssemblyAI and Speechmatics adapters behind feature flags.

## Acceptance Criteria

- [x] AssemblyAI provider is gated by cargo feature `assemblyai`.
- [x] Speechmatics provider is gated by cargo feature `speechmatics`.
- [x] AssemblyAI adapter maps transcript metadata into provider-neutral result fields or `provider_options`.
- [x] Speechmatics adapter maps multilingual capabilities into provider-neutral metadata.
- [x] Batch/result shape differences are documented.
- [x] Each adapter's `transcribe()` and `start_stream()` support matrix is explicit and tested.
- [x] Fake tests cover routing, unsupported options and metadata preservation.
- [x] Env-gated live tests are documented with required variable names and ignored by default.

## Implementation Notes

- AssemblyAI is implemented as `assemblyai/universal` behind feature `assemblyai`. `transcribe()` supports `AudioInput::Url` directly and uploads `AudioInput::Bytes` / `AudioInput::File` through the provider upload endpoint before submit+poll. `start_stream()` returns `UnsupportedOperation`.
- AssemblyAI maps provider metadata into neutral result fields: transcript text, detected/requested language, confidence, word timestamps, speaker utterances, audio duration and usage. Provider options include `speaker_labels`, `language_detection`, `language_confidence_threshold`, `speech_model` and `format_text`.
- Speechmatics is implemented as `speechmatics/enhanced` and `speechmatics/standard` behind feature `speechmatics`. `transcribe()` supports `AudioInput::Bytes` / `AudioInput::File` via batch multipart submit+poll+transcript fetch. URL fetch is explicitly unsupported in this adapter for v0.9.6 and tested as `UnsupportedOperation`; `start_stream()` also returns `UnsupportedOperation`.
- Speechmatics maps JSON-v2 transcript results into text, language, confidence, word timestamps and coalesced speaker segments. Provider options include `operating_point`, `diarization`, `additional_vocab` and `enable_entities`.
- Live test gates:
  - AssemblyAI required: `ASSEMBLYAI_API_KEY`, `ASSEMBLYAI_ASR_AUDIO_URL`
  - AssemblyAI optional: `ASSEMBLYAI_ASR_MODEL` (default `universal`), `ASSEMBLYAI_ASR_API_URL`, `ASSEMBLYAI_ASR_UPLOAD_URL`
  - Speechmatics required: `SPEECHMATICS_API_KEY`, `SPEECHMATICS_ASR_AUDIO_FILE`
  - Speechmatics optional: `SPEECHMATICS_ASR_MODEL` (default `enhanced`), `SPEECHMATICS_ASR_API_URL`
