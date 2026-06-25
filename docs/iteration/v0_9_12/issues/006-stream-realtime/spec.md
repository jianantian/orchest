# Issue 006: Stream impl + realtime absorption

## Background

ASR/TTS streaming and the omni realtime path are all openspeech / minimax-WS dialects. This issue builds
`orchest-provider-stream` (the WebSocket weight tier) and moves them in as `Asr` / `Tts` / `RealtimeSession`
impls, **deleting `agent-runtime-realtime-providers`** — its hand-rolled `RealtimeError` and event enum are
replaced by the protocol's unified error + unified event model.

## Goal / Scope

In scope:

- `orchest-provider-stream`: openspeech (Volcengine asr/tts/omni), minimax-ws (tts), streaming-ASR dialects
  (deepgram/soniox/speechmatics/assemblyai/aliyun) as `Asr`/`Tts` impls.
- Omni implemented as `RealtimeSession` reusing `ContentBlock`/`ToolUse` + the unified event model; auth via
  L1 `X-Api-*` header strategies (Issue 003).
- Delete `agent-runtime-realtime-providers`; register everything via the wall.

Out of scope:

- One-shot REST ASR → `orchest-provider-http` (here if trivial, else Issue 007).
- Visual gen (Issue 007).

## Acceptance Criteria

- [ ] The openspeech binary protocol exists **once**, shared by asr/tts/omni.
- [ ] Omni runs as `RealtimeSession` (`send(SessionInput)`, pulled `events()`); the **omni ruler** passes
      against a fake session — audio in / audio+text out / mid-stream tool use, audio never blocks.
- [ ] `agent-runtime-realtime-providers` is deleted; no provider-local `RealtimeError`/event enum remains.
- [ ] ASR/TTS streaming behavior preserved (existing tests green).

## Notes

Depends on 002/003/004. This is where the v0.9.11 realtime scaffold is finally folded into the protocol.
