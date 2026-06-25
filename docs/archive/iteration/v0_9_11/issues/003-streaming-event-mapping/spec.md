# Issue 003: Streaming event mapping

## Background

Omni/realtime providers produce interleaved events: audio chunks, text deltas, transcripts, lifecycle events and sometimes tool-use-like intent. v0.9.11 must observe these shapes without collapsing them into ASR, TTS or turn-based LLM semantics.

## Goal / Scope

Map selected-provider inbound/outbound realtime events into shared content/event structures where practical, while recording any gaps that require future provider-unification design.

In scope:

- Distinguish audio output, text delta, transcript, lifecycle, error and optional tool-use events.
- Reuse `agent-runtime-model::ContentBlock` modality concepts where they fit.
- Preserve provider-specific event details behind explicit metadata or internal event variants when required.
- Add fake-event tests covering interleaving order.

Out of scope:

- Do not force all events into existing ASR/TTS/LLM event names if semantics differ.
- Do not invent final cross-provider realtime event taxonomy beyond this experimental path.
- Do not require native tool-use support if the selected provider lacks it.

## Acceptance Criteria

- [x] Fake tests cover at least audio output, text/transcript output, lifecycle events and error events.
- [x] Event mapping preserves ordering for interleaved audio/text output.
- [x] Provider tool-use support is surfaced if native, or explicitly marked unsupported/emulated if not native.
- [x] Unsupported event shapes are recorded for the evidence report instead of silently dropped.
- [x] The implementation remains experimental/feature-gated and does not stabilize final unification APIs.

## Notes

Issue 003 depends on Issue 001 protocol notes and Issue 002 lifecycle scaffolding. This issue is about observation fidelity. Prefer explicit limitations over over-generalized abstractions.
