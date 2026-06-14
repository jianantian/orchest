# 006 · ASR provider docs and examples

## Background

Provider expansion needs usable examples so downstream SDK users can select models and understand unsupported behavior.

## Goal

Document ASR provider selection, one-shot transcription and streaming examples.

## Acceptance Criteria

- [ ] ASR guide documents `"provider/model"` selector examples for all new providers.
- [ ] Examples cover `transcribe()` and `start_stream()`.
- [ ] Unsupported-operation behavior is shown for realtime-only providers where applicable.
- [ ] Examples show byte, file and URL one-shot inputs.
- [ ] Examples show feature flags for each provider.
- [ ] Live-test environment variables are documented.
- [ ] Examples compile or run under the same CI command used by the provider crate.
