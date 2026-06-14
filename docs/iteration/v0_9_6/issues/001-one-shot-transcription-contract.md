# 001 · One-shot transcription contract

## Background

v0.9.1 reserved `AsrProvider::transcribe()` but allowed adapters to return unsupported. The public shape now needs real semantics before adding more providers.

## Goal

Define and implement one-shot transcription behavior for complete-audio inputs.

## Acceptance Criteria

- [ ] Request types support file URL/path and byte input behavior explicitly.
- [ ] Realtime-only providers either return `UnsupportedOperation` or drive `start_stream()` internally; the choice is documented per provider.
- [ ] Timeout and cancellation behavior is specified and tested.
- [ ] Gateway routing works for explicit model and auto-selected provider paths.
- [ ] Fake-provider tests cover success, unsupported and cancellation.
