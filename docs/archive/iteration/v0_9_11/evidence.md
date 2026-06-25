# v0.9.11 Omni Realtime Evidence Report

## Status

Date: 2026-06-26

This report captures the evidence produced by v0.9.11 Issues 001-005 so far. The implementation is an experimental provider-local surface in a dedicated realtime provider crate; it is not a final provider-core abstraction and does not merge existing modality provider crates.

## Provider Decision

First provider: **Doubao / Volcengine realtime**.

Reason: checked-in vendor documentation exists, the protocol exercises the desired realtime shape, and existing Volcengine provider code makes duplicated auth/transport/error pressure observable.

Second provider: **deferred** until after the first Volcengine realtime session produces enough live evidence.

See `docs/archive/iteration/v0_9_11/provider-decision.md` for endpoint, auth headers, event IDs, audio contract and credential variables.

## Implemented Experimental Surface

Implementation location:

- `crates/agent-runtime-realtime-providers/src/providers/volcengine/realtime/mod.rs`
- `crates/agent-runtime-realtime-providers/src/providers/volcengine/realtime/live.rs`
- `crates/agent-runtime-realtime-providers/src/providers/volcengine/mod.rs`

The module provides:

- `VolcengineRealtimeConfig` with defaults and `from_env()` for the Issue 001 credential contract.
- StartSession payload generation for deterministic `audio_file` input and `pcm_s16le` TTS output.
- `VolcengineRealtimeSession::fake()` lifecycle scaffolding for start, audio input and close.
- Experimental mapped event types for lifecycle, transcript, model text, audio output, metadata, errors and unsupported events.
- `ClientInterrupt` handling constrained to `push_to_talk` mode.
- Debuggable error classification for authentication, transport, protocol and provider failures.

Live WebSocket transport code now exists for credential-gated validation, and `examples/rust/realtime/volcengine_realtime_live.rs` provides a runnable real-API example. Live sessions were executed on 2026-06-25 and 2026-06-26 with repository `.env` credentials using `VOLCENGINE_APP_ID`, `VOLCENGINE_ACCESS_TOKEN` and `VOLCENGINE_REALTIME_RESOURCE_ID`. The smoke run established the WebSocket handshake, captured `X-Tt-Logid`, observed `ConnectionStarted`, `SessionStarted` and `SessionFinished`, and sent the checked-in 20ms silence PCM fixture. A generated-speech run sent real PCM input and observed final ASR, model text, PCM TTS audio output, `TTSEnded`, `UsageResponse` metadata, TTS audio file persistence and clean session close.

## Validation Commands and Outcomes

### Static fixture validation

```bash
python -m json.tool docs/archive/iteration/v0_9_11/fixtures/start_session_audio_file_o2.json
python -m json.tool docs/archive/iteration/v0_9_11/fixtures/text_query.json
python -m json.tool docs/archive/iteration/v0_9_11/fixtures/fake_event_sequence.json
stat -c '%n %s bytes' docs/archive/iteration/v0_9_11/fixtures/silence_20ms_16k_s16le.pcm
```

Outcome: all JSON fixtures parsed successfully; silence fixture is 640 bytes.

### Targeted Rust validation

```bash
cargo test -p agent-runtime-realtime-providers realtime --features volcengine
```

Outcome: passed. Fifteen realtime-focused tests covered config defaults, shared Volcengine environment variables, rejection of invented realtime credential names, WebSocket handshake header construction, StartConnection/StartSession frame shape, fake lifecycle, audio-before-start rejection, fake event mapping, `UsageResponse` metadata mapping, non-blocking audio input acknowledgement, live-session event stream hygiene, push-to-talk-only interruption and error classification. One additional ignored live test covers credential-gated WebSocket connection, audio send and event receive when environment variables are configured. The live example compiles with `cargo check -p agent-runtime-realtime-providers --features volcengine --example volcengine_realtime_live`.

### Live Volcengine realtime validation

```bash
set -a; source .env; set +a; cargo run -p agent-runtime-realtime-providers --features volcengine --example volcengine_realtime_live
```

Provider/model: Volcengine realtime, default model `1.2.1.1`, default speaker `zh_female_vv_jupiter_bigtts`.

Date: 2026-06-25.

Outcome: connected successfully. The run printed `X-Tt-Logid: 20260625232232A69AB303AEC30A9CE761`, started session `7197ce0d-8140-4b8b-8f2a-d3bb82bf4c24`, sent the checked-in 640-byte 16kHz mono PCM silence fixture, observed lifecycle events `ConnectionStarted`, `SessionStarted` and `SessionFinished`, and closed the session plus WebSocket connection cleanly. No transcript, model text or audio output was observed because the input was only one 20ms silence frame.

### Live Volcengine realtime validation with generated speech

```bash
say -o /tmp/orchest_realtime_live.aiff "Hello, this is Orchest realtime validation."
ffmpeg -y -hide_banner -loglevel error -i /tmp/orchest_realtime_live.aiff -ac 1 -ar 16000 -f s16le /tmp/orchest_realtime_live_16k_s16le.pcm
set -a; source .env; set +a; cargo run -p agent-runtime-realtime-providers --features volcengine --example volcengine_realtime_live -- /tmp/orchest_realtime_live_16k_s16le.pcm
```

Provider/model: Volcengine realtime, default model `1.2.1.1`, default speaker `zh_female_vv_jupiter_bigtts`.

Date: 2026-06-26.

Outcome: connected successfully. The run printed `X-Tt-Logid: 202606260010212FF4124A6BDD85999F3C`, started session `68bb8a31-f5fc-46c5-9057-0fdc85f03552`, appended 2000 ms of trailing PCM silence to the generated speech sample, and sent 161,426 bytes of 16 kHz mono PCM. The provider returned `ASRInfo`, 49 `ASRResponse` transcript events, one final transcript (`Hello, this orchestra. Time validation.`), `ASREnded`, 5 `ChatResponse` text deltas (`Hi! What can I do for you?`), `ChatEnded`, 7 `TTSResponse` PCM audio chunks totaling 114,674 bytes, `UsageResponse`, `TTSSentenceEnd`, `TTSEnded`, `SessionFinished` and clean WebSocket shutdown. The example wrote the returned 24 kHz `pcm_s16le` TTS audio to `/tmp/orchest_realtime_live_tts_24k_s16le.pcm` (112K on disk).

### Repository lint check

```bash
bash scripts/lint-check.sh
```

Outcome: passed. The script reported existing file-length warnings unrelated to this change.

## Observed Capabilities

| Capability | Observed in fake/session evidence | Live provider observed |
|------------|-----------------------------------|------------------------|
| Audio input | Yes: fake session accepts a 640-byte PCM chunk; live path can send audio through WebSocket command loop | Yes: sent one 640-byte PCM silence chunk and a 161,426-byte generated speech PCM stream including trailing silence |
| Audio output | Yes: fake event sequence maps `TTSResponse` to audio output bytes | Yes: observed 114,674 bytes of PCM TTS audio across 7 `TTSResponse` chunks and wrote them to `/tmp/orchest_realtime_live_tts_24k_s16le.pcm` |
| Text/transcript output | Yes: fake event sequence maps `ASRResponse` and `ChatResponse` | Yes: observed interim and final ASR transcript events plus model text deltas |
| Lifecycle events | Yes: start, close, mapped provider lifecycle events and live StartConnection/StartSession/FinishSession frames | Yes: observed `ConnectionStarted`, `SessionStarted` and `SessionFinished` |
| Interruption | Yes: `ClientInterrupt` is accepted only for `push_to_talk` fake semantics | Not run in live validation |
| Tool use | Not supported / not observed in checked-in Volcengine docs | Not observed |

## Provider-Specific Quirks

- The provider uses a custom binary WebSocket protocol instead of plain JSON messages.
- Session-level events carry optional session identifiers in binary-frame optional fields.
- `ClientInterrupt` is documented for `push_to_talk` mode, so it should not be generalized as universal realtime cancellation.
- Native tool-call events are not described in the checked-in vendor documentation.
- Deterministic tests should request PCM output; the provider default output is OGG/Opus.
- `UsageResponse` arrives as provider metadata (`event_id=154`) after TTS audio chunks and before final TTS lifecycle events.

## Refactor Inputs for Provider Unification

1. **Realtime duplex is a separate interaction primitive.** It should not be modeled as ASR -> LLM -> TTS; the current implementation therefore lives in a dedicated experimental realtime provider crate instead of an ASR provider module.
2. **Event mapping needs a shared vocabulary without erasing provider detail.** Lifecycle, transcript, model text, audio output, metadata and provider errors need common concepts plus provider-specific payload escape hatches.
3. **Cancellation/interruption must be capability-flagged.** `ClientInterrupt` is mode-limited for Volcengine; a future abstraction should distinguish close, cancel, barge-in and provider-specific interruption.
4. **Provider-core candidates are visible but not ready.** Auth/config defaults, WebSocket binary framing, error classification and telemetry/log IDs are likely shared infrastructure candidates, but this iteration should not stabilize them yet.
5. **Tool-use support must be evidence-based.** The first provider does not document native tool calls, so a future capability flag should represent unsupported/native/emulated states.

## Evidence Gaps

- No provider error body or live `ClientInterrupt` behavior was observed.
- Qwen omni has not been compared because official docs have not been added under `docs/external/`.

## Follow-Up

- Before provider unification, run the ignored credential-gated live test or `cargo run -p agent-runtime-realtime-providers --features volcengine --example volcengine_realtime_live -- /path/to/16k_s16le_mono.pcm` with a speech-containing sample when provider behavior changes, then append any changed transcript/audio/tool-use outcomes to this report.
- Update `docs/todo/provider-unification.md` only if live evidence changes the current Step 2 assumptions.
