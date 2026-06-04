---
name: project-asr-provider-backlog
description: Future ASR provider backlog after v0.9.1; keep v0.9.1 focused on Volcengine and Aliyun while preserving interface pressure from broader STT APIs
metadata:
  node_type: memory
  type: project
  originSessionId: orchest-v0-9-1-asr-review
---

v0.9.1 intentionally implements only Volcengine and Aliyun for `agent-runtime-asr-providers`. This keeps the first ASR gateway iteration focused on domestic realtime ASR integrations while still forcing the abstraction to work across two different vendor APIs.

**Why:** Additional providers are useful, but not needed to validate the first crate boundary. They should not expand v0.9.1 scope. Keep them as follow-up provider work once the core ASR contract, duplex streaming API, routing, compatibility policy, telemetry and error model are stable.

**How to apply:**
- Do not add these providers to v0.9.1 acceptance criteria.
- Use them as design pressure when reviewing public ASR types, especially `AsrStream`, `AsrStreamEvent`, `TranscribeOptions`, `AsrModelCapabilities`, `OptionAdjustment` and provider/model normalization.
- Future provider adapters must use the same `"provider/model"` convention as `agent-runtime-providers`.
- Provider-specific power goes through typed config when it is common across providers, otherwise through `provider_options`.

**API backlog:**
- **Implement `transcribe()` one-shot transcription**: v0.9.1 reserves the public `transcribe()` signature and request/result/error shape, but does not require provider adapters to implement batch or complete-audio transcription. A follow-up iteration should define provider behavior for file URLs, byte inputs, realtime-only providers, batch-capable providers, timeout/cancellation, and whether realtime-only adapters may implement `transcribe()` by internally driving `start_stream()` to `Final`.

**Provider backlog:**
- **ElevenLabs Scribe v2 Realtime**: High-priority follow-up. Good interface calibration target because it supports realtime WebSocket STT, low-latency partials, word-level timestamps, keyterm prompting, language detection and diarization-related semantics. Model examples: `elevenlabs/scribe_v2_realtime`, `elevenlabs/scribe_v2`.
- **Deepgram**: Strong realtime STT baseline with established voice-agent usage. Useful for latency, endpointing and streaming event comparison.
- **Soniox**: Useful for multilingual and code-switching pressure on `Language`, `code_switching` and routing.
- **AssemblyAI**: Useful for rich transcript metadata and batch/transcript result shape pressure.
- **Speechmatics**: Useful multilingual fallback and enterprise STT comparison point.

Related: `docs/iteration/v0_9_1/prd.md`
