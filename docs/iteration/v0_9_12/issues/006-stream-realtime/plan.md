# Issue 006 Plan: Stream impl + realtime absorption

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (omni ruler), `docs/adr/0001-provider-unification.md`
- `crates/agent-runtime-asr-providers/src/providers/{volcengine,aliyun,deepgram,soniox,speechmatics,assemblyai,elevenlabs}/`
- `crates/agent-runtime-tts-providers/src/providers/{volcengine,minimax,aliyun}/`
- `crates/agent-runtime-realtime-providers/src/providers/volcengine/realtime/` (mod.rs, live.rs, tests.rs)
- `orchest-provider-core` WS scaffold + binary-frame codec + L1 `X-Api-*` strategies (Issue 003 output)

## Files to Change

- `crates/orchest-provider-stream/src/` (skeleton from Issue 004, `ws` feature): openspeech dialect shared by asr/tts/omni; minimax-ws; streaming-ASR dialects; WS-backed `synthesize()` (Minimax sync WSS, Volcengine unidirectional)
- `RealtimeSession` impl for omni
- `crates/orchest-provider-http/src/` (skeleton from Issue 004): REST/SSE one-shot `transcribe()` + REST TTS modules
- Delete `crates/agent-runtime-realtime-providers`; remove from workspace
- Wall registration (Issue 004)

## Steps

1. Use the `orchest-provider-stream` skeleton (from Issue 004); ensure it depends on protocol + core.
2. Port the openspeech binary protocol once; build the Volcengine asr/tts dialect on it.
3. Implement omni as `RealtimeSession` over the same openspeech protocol; map frames → unified event model.
4. Port minimax-ws TTS and the per-vendor streaming-ASR dialects; keep WS-backed `synthesize()` (Minimax sync WSS, Volcengine unidirectional) in `-stream`.
5. Migrate REST/SSE one-shot `transcribe()` + REST TTS into `orchest-provider-http` (skeleton from 004) as `Asr`/`Tts` impls.
6. Delete `agent-runtime-realtime-providers`; drop its `RealtimeError`/event enum.
7. Register stream + http ASR/TTS entries through the wall.
8. Port the fake-session lifecycle tests; add the omni-ruler fake test (audio + tool use concurrency); fmt/clippy/test.
