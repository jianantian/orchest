# 006 · ASR provider docs and examples

## Background

Provider expansion needs usable examples so downstream SDK users can select models and understand unsupported behavior.

## Goal

Document ASR provider selection, one-shot transcription and streaming examples.

## Acceptance Criteria

- [x] ASR guide documents `"provider/model"` selector examples for all new providers.
- [x] Examples cover `transcribe()` and `start_stream()`.
- [x] Unsupported-operation behavior is shown for realtime-only providers where applicable.
- [x] Examples show byte, file and URL one-shot inputs.
- [x] Examples show feature flags for each provider.
- [x] Live-test environment variables are documented.
- [x] Examples compile or run under the same CI command used by the provider crate.

## Implementation Notes

- Added `docs/iteration/v0_9_6/asr-provider-guide.md` with selector, feature flag, support matrix, one-shot input, streaming, live-test environment and verification sections.
- Added compile-checked `asr_transcribe` Rust example. It uses `create_asr_provider_from_config()` and env vars to choose `AudioInput::Bytes`, `AudioInput::File` or `AudioInput::Url` without hard-coding a provider type in the example.
- Existing streaming examples remain `asr_full_duplex` and `asr_segmented`; the guide references both for `start_stream()` workflows.
- Realtime-only `transcribe()` and batch-only `start_stream()` behavior are documented as `AsrErrorCode::UnsupportedOperation`.
