# v0.9.11 Omni Realtime Evidence Report

## Status

Date: 2026-06-25

This report captures the evidence produced by v0.9.11 Issues 001-005 so far. The implementation is an experimental provider-local surface under the existing Volcengine ASR provider module; it is not a final provider-core abstraction and does not merge provider crates.

## Provider Decision

First provider: **Doubao / Volcengine realtime**.

Reason: checked-in vendor documentation exists, the protocol exercises the desired realtime shape, and existing Volcengine provider code makes duplicated auth/transport/error pressure observable.

Second provider: **deferred** until after the first Volcengine realtime session produces enough live evidence.

See `docs/iteration/v0_9_11/provider-decision.md` for endpoint, auth headers, event IDs, audio contract and credential variables.

## Implemented Experimental Surface

Implementation location:

- `crates/agent-runtime-asr-providers/src/providers/volcengine/realtime.rs`
- `crates/agent-runtime-asr-providers/src/providers/volcengine/mod.rs`

The module provides:

- `VolcengineRealtimeConfig` with defaults and `from_env()` for the Issue 001 credential contract.
- StartSession payload generation for deterministic `audio_file` input and `pcm_s16le` TTS output.
- `VolcengineRealtimeSession::fake()` lifecycle scaffolding for start, audio input and close.
- Experimental mapped event types for lifecycle, transcript, model text, audio output, metadata, errors and unsupported events.
- `ClientInterrupt` handling constrained to `push_to_talk` mode.
- Debuggable error classification for authentication, transport, protocol and provider failures.

Live WebSocket transport code now exists for credential-gated validation, and `examples/rust/asr/volcengine_realtime_live.rs` provides a runnable real-API example. No live session was executed in this environment because credentials are not configured. The current verified surface is fake-session scaffolding plus mapping/error semantics; live transport remains covered by an ignored test and the live example until credentials are configured.

## Validation Commands and Outcomes

### Static fixture validation

```bash
python -m json.tool docs/iteration/v0_9_11/fixtures/start_session_audio_file_o2.json
python -m json.tool docs/iteration/v0_9_11/fixtures/text_query.json
python -m json.tool docs/iteration/v0_9_11/fixtures/fake_event_sequence.json
stat -c '%n %s bytes' docs/iteration/v0_9_11/fixtures/silence_20ms_16k_s16le.pcm
```

Outcome: all JSON fixtures parsed successfully; silence fixture is 640 bytes.

### Targeted Rust validation

```bash
cargo test -p agent-runtime-asr-providers realtime --features volcengine
```

Outcome: passed. Seven realtime-focused tests covered config defaults, StartSession payload shape, fake lifecycle, audio-before-start rejection, fake event mapping, push-to-talk-only interruption and error classification. One additional ignored live test covers credential-gated WebSocket connection, audio send and event receive when environment variables are configured. The live example compiles with `cargo check -p agent-runtime-asr-providers --features volcengine --example volcengine_realtime_live`.

### Repository lint check

```bash
bash scripts/lint-check.sh
```

Outcome: passed. The script reported existing file-length warnings unrelated to this change.

## Observed Capabilities

| Capability | Observed in fake/session evidence | Live provider observed |
|------------|-----------------------------------|------------------------|
| Audio input | Yes: fake session accepts a 640-byte PCM chunk; live path can send audio through WebSocket command loop | Not run; credentials unavailable in this environment |
| Audio output | Yes: fake event sequence maps `TTSResponse` to audio output bytes | Not run; credentials unavailable in this environment |
| Text/transcript output | Yes: fake event sequence maps `ASRResponse` and `ChatResponse` | Not run; credentials unavailable in this environment |
| Lifecycle events | Yes: start, close, mapped provider lifecycle events and live StartConnection/StartSession/FinishSession frames | Not run; credentials unavailable in this environment |
| Interruption | Yes: `ClientInterrupt` is accepted only for `push_to_talk` fake semantics | Not run; credentials unavailable in this environment |
| Tool use | Not supported / not observed in checked-in Volcengine docs | Not run; no vendor evidence yet |

## Provider-Specific Quirks

- The provider uses a custom binary WebSocket protocol instead of plain JSON messages.
- Session-level events carry optional session identifiers in binary-frame optional fields.
- `ClientInterrupt` is documented for `push_to_talk` mode, so it should not be generalized as universal realtime cancellation.
- Native tool-call events are not described in the checked-in vendor documentation.
- Deterministic tests should request PCM output; the provider default output is OGG/Opus.

## Refactor Inputs for Provider Unification

1. **Realtime duplex is a separate interaction primitive.** It should not be modeled as ASR -> LLM -> TTS even though the first experimental module lives in the ASR provider crate for dependency locality.
2. **Event mapping needs a shared vocabulary without erasing provider detail.** Lifecycle, transcript, model text, audio output, metadata and provider errors need common concepts plus provider-specific payload escape hatches.
3. **Cancellation/interruption must be capability-flagged.** `ClientInterrupt` is mode-limited for Volcengine; a future abstraction should distinguish close, cancel, barge-in and provider-specific interruption.
4. **Provider-core candidates are visible but not ready.** Auth/config defaults, WebSocket binary framing, error classification and telemetry/log IDs are likely shared infrastructure candidates, but this iteration should not stabilize them yet.
5. **Tool-use support must be evidence-based.** The first provider does not document native tool calls, so a future capability flag should represent unsupported/native/emulated states.

## Evidence Gaps

- No credential-gated live run was executed in this environment; `live_realtime_session_can_send_audio_and_receive_events` is ignored by default and requires Volcengine realtime credentials.
- No real audio output bytes from Volcengine were captured.
- No real `X-Tt-Logid`, provider error body or live `ClientInterrupt` behavior was observed.
- Qwen omni has not been compared because official docs have not been added under `docs/external/`.

## Follow-Up

- Before provider unification, run the ignored credential-gated live test or `cargo run -p agent-runtime-asr-providers --features volcengine --example volcengine_realtime_live`, then append the exact command, provider model, date, log ID and outcomes to this report.
- Update `docs/todo/provider-unification.md` only if live evidence changes the current Step 2 assumptions.
