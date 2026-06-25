# v0.9.11 Provider Decision: Doubao / Volcengine Realtime

## Decision

Use **Doubao / Volcengine realtime** as the first v0.9.11 omni/realtime provider target.

A second provider is **deferred**. Qwen omni remains a candidate for a later comparison only after official vendor documentation is added under `docs/external/` and the first Volcengine realtime session has produced enough implementation evidence.

## Rationale

- Checked-in vendor documentation already exists at `docs/external/volceengine/realtime.md`, so implementation can proceed without adding external source material first.
- The provider exposes the target shape for this iteration: WebSocket realtime transport, streaming client input, server audio output, transcript/text events, session lifecycle events and interruption events.
- Volcengine is already represented in existing LLM/ASR/TTS provider work, which makes duplicated auth, telemetry, error and transport pressure observable for the later provider-unification refactor.

## Vendor Protocol Notes

### Transport and Auth

- Endpoint: `wss://openspeech.bytedance.com/api/v3/realtime/dialogue`.
- Required request headers:
  - `X-Api-App-ID`: Volcengine app ID.
  - `X-Api-Access-Key`: access token.
  - `X-Api-Resource-Id`: fixed value `volc.speech.dialog`.
  - `X-Api-App-Key`: fixed value `PlgvMymc7f3tQnJ6`.
- Optional tracing header:
  - `X-Api-Connect-Id`: caller-generated connection ID.
- Response header to log for debugging:
  - `X-Tt-Logid`.

### Binary Frame Model

The WebSocket payload uses a binary protocol with a 4-byte header, optional fields, payload size and payload.

Important message types for v0.9.11:

| Direction | Message type | Meaning |
|-----------|--------------|---------|
| client → server | `0b0001` | Full-client request / JSON event |
| server → client | `0b1001` | Full-server response / JSON event |
| client → server | `0b0010` | Audio-only request |
| server → client | `0b1011` | Audio-only response |
| server → client | `0b1111` | Error information |

Payloads may be JSON event bodies or raw audio bytes. Session-level events carry a session ID in the optional fields.

### Client Events in Scope

| Event ID | Name | v0.9.11 use |
|----------|------|-------------|
| 1 | `StartConnection` | Open logical realtime connection after WebSocket upgrade |
| 2 | `FinishConnection` | Close logical connection when not reusing the WebSocket |
| 100 | `StartSession` | Start one realtime session and configure `asr`, `dialog`, and `tts` |
| 102 | `FinishSession` | End a session while keeping the WebSocket reusable |
| 200 | `TaskRequest` | Upload client audio chunks |
| 515 | `ClientInterrupt` | Provider-supported interruption in `push_to_talk` mode |

### Server Events in Scope

| Event ID | Name | Event class for Orchest evidence |
|----------|------|----------------------------------|
| 50 | `ConnectionStarted` | lifecycle |
| 51 | `ConnectionFailed` | error / lifecycle |
| 52 | `ConnectionFinished` | lifecycle |
| 150 | `SessionStarted` | lifecycle |
| 152 | `SessionFinished` | lifecycle |
| 153 | `SessionFailed` | error / lifecycle |
| 350 | `TTSSentenceStart` | text / audio-generation metadata |
| 351 | `TTSSentenceEnd` | text / audio-generation metadata |
| 352 | `TTSResponse` | audio output bytes |
| 359 | `TTSEnded` | audio-generation lifecycle |
| 450 | `ASRInfo` | input-speech detected / interruption signal |
| 451 | `ASRResponse` | transcript delta/result |
| 459 | `ASREnded` | transcript lifecycle |
| 550 | `ChatResponse` | model text delta/content |
| 559 | `ChatEnded` | model text lifecycle |

### Audio and Input Modes

- Preferred deterministic fixture mode: `audio_file` input mode.
- Live microphone mode should stream 20 ms audio chunks where possible.
- For PCM input, the vendor requirement is mono, 16 kHz, signed 16-bit little-endian PCM.
- A 20 ms PCM chunk at 16 kHz / 16-bit / mono is 640 bytes.
- Default server audio output is OGG/Opus. v0.9.11 should request PCM output in `StartSession` where deterministic tests need raw bytes.
- PCM TTS output options documented by the vendor include mono 24 kHz `pcm` or `pcm_s16le`.

## Credential Contract

Use these environment variable names for manual validation unless implementation issue 002 has a strong reason to rename them:

| Environment variable | Maps to |
|----------------------|---------|
| `VOLCENGINE_REALTIME_APP_ID` | `X-Api-App-ID` |
| `VOLCENGINE_REALTIME_ACCESS_KEY` | `X-Api-Access-Key` |
| `VOLCENGINE_REALTIME_RESOURCE_ID` | `X-Api-Resource-Id`; default `volc.speech.dialog` |
| `VOLCENGINE_REALTIME_APP_KEY` | `X-Api-App-Key`; default `PlgvMymc7f3tQnJ6` |
| `VOLCENGINE_REALTIME_CONNECT_ID` | Optional `X-Api-Connect-Id` override |
| `VOLCENGINE_REALTIME_MODEL` | StartSession `dialog.extra.model`; default `1.2.1.1` for O2.0 |
| `VOLCENGINE_REALTIME_SPEAKER` | StartSession `tts.speaker`; default `zh_female_vv_jupiter_bigtts` |

## Tool-Use Decision

The checked-in Volcengine realtime documentation does not describe native tool-call events. v0.9.11 should therefore mark native tool use as **unsupported / not observed** for this first provider unless live validation or updated vendor docs prove otherwise.

## Interruption Decision

`ClientInterrupt` is documented for `push_to_talk` mode. v0.9.11 should implement or fake-test the frame path when using that mode, and otherwise document interruption as mode-limited rather than universal realtime cancellation.
