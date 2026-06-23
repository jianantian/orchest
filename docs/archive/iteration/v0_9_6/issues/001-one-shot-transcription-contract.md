# 001 · One-shot transcription contract

## Background

v0.9.1 reserved `AsrProvider::transcribe()` but allowed adapters to return unsupported. The public shape now needs real semantics before adding more providers.

## Goal

Define and implement one-shot transcription behavior for complete-audio inputs.

## Acceptance Criteria

- [x] `TranscribeRequest.audio` documents and tests `AudioInput::Bytes`, `AudioInput::File` and `AudioInput::Url`.
- [x] Byte input requires an explicit `AudioFormat`; file and URL inputs either use explicit format or provider-supported inference.
- [x] Realtime-only providers either return `UnsupportedOperation` or drive `start_stream()` internally; the choice is documented per provider.
- [x] Timeout behavior is specified for gateway routing and provider execution, including the returned `AsrErrorCode`.
- [x] Cancellation behavior is specified and tested for in-flight one-shot requests.
- [x] Gateway routing works for explicit model and auto-selected provider paths.
- [x] Fake-provider tests cover success, unsupported, timeout and cancellation.
- [x] Public examples that call `transcribe()` use the finalized request shape.

## Notes

Implemented contract:

- `TranscribeRequest.timeout` applies after routing and compatibility validation, around provider execution. Timeout returns `AsrErrorCode::Timeout`.
- Cancelling the gateway `transcribe()` future drops the provider future; providers should keep cancellation cleanup inside the returned future rather than detached background tasks.
- Current Volcengine and Aliyun adapters remain realtime-only for one-shot calls. Gateway validation returns `AsrErrorCode::UnsupportedOperation` for their `transcribe()` path because their capabilities advertise `batch = false`.
- `AudioInput::Bytes` requires non-empty bytes and an explicit `AudioFormat`; raw PCM bytes also require `sample_rate_hz`.
- `AudioInput::File` and `AudioInput::Url` require an explicit format unless the selected provider advertises `batch_format_inference = true`.
