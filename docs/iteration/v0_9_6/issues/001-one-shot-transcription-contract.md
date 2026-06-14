# 001 · One-shot transcription contract

## Background

v0.9.1 reserved `AsrProvider::transcribe()` but allowed adapters to return unsupported. The public shape now needs real semantics before adding more providers.

## Goal

Define and implement one-shot transcription behavior for complete-audio inputs.

## Acceptance Criteria

- [ ] `TranscribeRequest.audio` documents and tests `AudioInput::Bytes`, `AudioInput::File` and `AudioInput::Url`.
- [ ] Byte input requires an explicit `AudioFormat`; file and URL inputs either use explicit format or provider-supported inference.
- [ ] Realtime-only providers either return `UnsupportedOperation` or drive `start_stream()` internally; the choice is documented per provider.
- [ ] Timeout behavior is specified for gateway routing and provider execution, including the returned `AsrErrorCode`.
- [ ] Cancellation behavior is specified and tested for in-flight one-shot requests.
- [ ] Gateway routing works for explicit model and auto-selected provider paths.
- [ ] Fake-provider tests cover success, unsupported, timeout and cancellation.
- [ ] Public examples that call `transcribe()` use the finalized request shape.
