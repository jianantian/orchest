# v0.9.6 ASR Provider Guide

## Provider selectors and feature flags

All public ASR selectors use `"provider/model"` form.

| Selector | Feature | `transcribe()` | `start_stream()` | Notes |
| --- | --- | --- | --- | --- |
| `deepgram/nova-3` | `deepgram` | `UnsupportedOperation` | Supported | Realtime baseline with word timestamps and endpointing options. |
| `elevenlabs/scribe_v2_realtime` | `elevenlabs` | `UnsupportedOperation` | Supported | Realtime Scribe v2 adapter; `elevenlabs/scribe_v2` is catalog-only in v0.9.6. |
| `soniox/stt-rt-v5` | `soniox` | `UnsupportedOperation` | Supported | Realtime multilingual/code-switching adapter. |
| `assemblyai/universal` | `assemblyai` | Supported | `UnsupportedOperation` | Batch submit+poll adapter; URL inputs are submitted directly, bytes/files are uploaded first. |
| `speechmatics/enhanced` | `speechmatics` | Supported for bytes/files | `UnsupportedOperation` | Batch multipart submit+poll adapter; URL fetch is explicitly unsupported in v0.9.6. |
| `speechmatics/standard` | `speechmatics` | Supported for bytes/files | `UnsupportedOperation` | Same adapter with `standard` operating point. |

Default crate features remain `volcengine` and `aliyun`. Enable follow-up providers explicitly:

```bash
cargo test -p agent-runtime-asr-providers --features deepgram,elevenlabs,soniox,assemblyai,speechmatics
```

## One-shot transcription

The `asr_transcribe` example compiles under the provider crate examples and accepts all one-shot audio input shapes:

```bash
cargo run -p agent-runtime-asr-providers --example asr_transcribe --features assemblyai
```

URL input:

```bash
ASSEMBLYAI_API_KEY=... \
ASR_MODEL=assemblyai/universal \
ASR_AUDIO_URL=https://example.com/audio.wav \
ASR_AUDIO_FORMAT=wav \
cargo run -p agent-runtime-asr-providers --example asr_transcribe --features assemblyai
```

File input:

```bash
SPEECHMATICS_API_KEY=... \
ASR_MODEL=speechmatics/enhanced \
ASR_AUDIO_FILE=/tmp/audio.wav \
ASR_AUDIO_FORMAT=wav \
cargo run -p agent-runtime-asr-providers --example asr_transcribe --features speechmatics
```

Bytes input:

```bash
SPEECHMATICS_API_KEY=... \
ASR_MODEL=speechmatics/enhanced \
ASR_AUDIO_BYTES_FILE=/tmp/audio.wav \
ASR_AUDIO_FORMAT=wav \
cargo run -p agent-runtime-asr-providers --example asr_transcribe --features speechmatics
```

Provider-specific options can be passed as JSON:

```bash
ASR_PROVIDER_OPTIONS_JSON='{"diarization":"speaker","operating_point":"enhanced"}'
```

Realtime-only adapters return `AsrErrorCode::UnsupportedOperation` from `transcribe()`. The example matches that error and prints a support-matrix message instead of treating it as a panic.

## Streaming

Use `asr_full_duplex` for a continuous producer/consumer streaming shape:

```bash
DASHSCOPE_API_KEY=... \
cargo run -p agent-runtime-asr-providers --example asr_full_duplex
```

Use `asr_segmented` for push-to-talk style `flush_and_wait_final()` segments:

```bash
DASHSCOPE_API_KEY=... \
cargo run -p agent-runtime-asr-providers --example asr_segmented
```

Deepgram, ElevenLabs and Soniox are realtime-only in v0.9.6. AssemblyAI and Speechmatics are batch-only in v0.9.6, so `start_stream()` returns `UnsupportedOperation`.

## Live tests

Live tests are ignored by default and cost real provider usage.

| Provider | Test | Required env | Optional env |
| --- | --- | --- | --- |
| Deepgram | `live_deepgram_streaming_silence` | `DEEPGRAM_API_KEY` | `DEEPGRAM_ASR_MODEL`, `DEEPGRAM_ASR_WS_URL` |
| ElevenLabs | `live_elevenlabs_streaming_silence` | `ELEVENLABS_API_KEY` | `ELEVENLABS_ASR_MODEL`, `ELEVENLABS_ASR_WS_URL` |
| Soniox | `live_soniox_streaming_silence_with_code_switching` | `SONIOX_API_KEY` | `SONIOX_ASR_MODEL`, `SONIOX_ASR_WS_URL` |
| AssemblyAI | `live_assemblyai_transcribe_url` | `ASSEMBLYAI_API_KEY`, `ASSEMBLYAI_ASR_AUDIO_URL` | `ASSEMBLYAI_ASR_MODEL`, `ASSEMBLYAI_ASR_API_URL`, `ASSEMBLYAI_ASR_UPLOAD_URL` |
| Speechmatics | `live_speechmatics_transcribe_file` | `SPEECHMATICS_API_KEY`, `SPEECHMATICS_ASR_AUDIO_FILE` | `SPEECHMATICS_ASR_MODEL`, `SPEECHMATICS_ASR_API_URL` |

Run an ignored live test explicitly:

```bash
cargo test -p agent-runtime-asr-providers --features assemblyai live_assemblyai_transcribe_url -- --ignored
```

## Verification

The provider crate checks used for v0.9.6 are:

```bash
cargo test -p agent-runtime-asr-providers
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo test -p agent-runtime-asr-providers --examples
cargo test -p agent-runtime-asr-providers --features deepgram,elevenlabs,soniox,assemblyai,speechmatics
cargo clippy -p agent-runtime-asr-providers --features deepgram,elevenlabs,soniox,assemblyai,speechmatics -- -D warnings
```
