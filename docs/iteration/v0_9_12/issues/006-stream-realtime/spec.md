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
- **All remaining ASR/TTS migration, routed by wire (not by API shape):** REST/SSE-backed one-shot
  `transcribe()` and REST TTS → `orchest-provider-http`; **WS-backed** `synthesize()`/`transcribe()` (e.g.
  Minimax sync WSS, Volcengine unidirectional-WS `synthesize()`) stay in `orchest-provider-stream`, exposing
  the non-streaming `Tts::synthesize()`/`Asr::transcribe()` surface over WS — so no ASR/TTS path is unowned
  before cleanup.
- Omni implemented as `RealtimeSession` reusing `ContentBlock`/`ToolUse` + the unified event model; auth via
  L1 `X-Api-*` header strategies (Issue 003).
- Delete `agent-runtime-realtime-providers`; register everything via the wall.

Out of scope:

- LLM migration (Issue 005); visual/music gen (Issue 007).

## Acceptance Criteria

- [ ] The openspeech binary protocol exists **once**, shared by asr/tts/omni.
- [ ] Omni runs as `RealtimeSession` (`send(SessionInput)`, pulled `events()`); the **omni ruler** passes
      against a fake session — audio in / audio+text out / mid-stream tool use, audio never blocks.
- [ ] `agent-runtime-realtime-providers` is deleted; no provider-local `RealtimeError`/event enum remains.
- [ ] ASR/TTS streaming behavior preserved (existing tests green).
- [ ] Every ASR/TTS path (streaming, one-shot `transcribe()`, sync `synthesize()`) has a home (`stream` or
      `http`); no un-migrated `agent-runtime-{asr,tts}` path remains before Issue 008.

## Notes

Depends on 002/003/004. This is where the v0.9.11 realtime scaffold is finally folded into the protocol.
The REST/SSE one-shot paths land in `orchest-provider-http`; that crate (and `-stream`/`-visual`) is created
as an empty skeleton in Issue 004, so impl issues only add modules and there is no crate-creation race.
