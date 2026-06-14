# 006 · ASR provider docs and examples

## Background

Provider expansion needs usable examples so downstream SDK users can select models and understand unsupported behavior.

## Goal

Document ASR provider selection, one-shot transcription and streaming examples.

## Acceptance Criteria

- [ ] ASR guide documents `"provider/model"` selector examples for all new providers.
- [ ] Examples cover `transcribe()` and `start_stream()`.
- [ ] Unsupported-operation behavior is shown for realtime-only providers where applicable.
- [ ] Live-test environment variables are documented.
