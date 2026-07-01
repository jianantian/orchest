# 005 · Multimedia ingestion and audio output

## Background

v0.10 is the breadth demo. A text-only research agent leaves four satellite provider gateways (AIGC, ASR, TTS) and the multimodal image-input path undogfooded before v1.0 freezes their public APIs. This issue chains the modalities a real briefing tool naturally uses, so those gateways ship validated rather than untested.

The modalities are not added for coverage's sake: a research-briefing tool realistically ingests recorded interviews (ASR) and chart images (vision), and offers an audio version of its output (TTS).

## Goal

Wire ASR transcription, multimodal image input and TTS audio output into Briefing Desk through public gateway APIs, each with a deterministic fake provider so the offline smoke path covers the full multimedia flow.

## Acceptance Criteria

- [ ] An ASR-backed `transcribe_audio` tool is registered; when the corpus contains an audio source, the agent transcribes it and the transcript feeds the brief.
- [ ] Image sources in the corpus are read through `ContentBlock::Image` into a vision-capable model path; image-derived facts appear in the brief.
- [ ] A TTS-backed `synthesize_brief` tool produces an audio version of the final brief; it marks `has_side_effects: true` and is gated by a flag.
- [ ] TTS skip/deny path leaves no audio file; approve path writes exactly one audio file.
- [ ] A deterministic fake `Asr` impl, fake vision model path, and fake `Tts` impl exist so `--fake` smoke runs the full transcribe → read-image → write → synthesize flow with no network credentials.
- [ ] Each modality's events (tool call, asset produced) render in the event stream without panics.
- [ ] All three modalities are driven through public Orchest APIs only; no private module access.
- [ ] Any gateway friction (awkward construction, missing fake provider hook, confusing asset handling, event-surface gaps) is recorded in the validation report and classified by the v0.10 triage rule.

## Notes

This issue depends on issue 003 (runtime tool flow) for the base tool/approval/event scaffolding and on issue 001's mixed-media fixtures.

AIGC image generation is an **optional stretch**, not a requirement of this issue. Add a `generate_figure` tool only if the brief genuinely benefits from a generated diagram; a forced illustration produces weak validation signal and should be left out. AIGC video/music and audio-block-to-LLM input are out of scope (the `Audio` ContentBlock has no provider yet; ASR is the supported audio→text bridge).

Do not implement new provider adapters. Use the existing gateways. If a gateway cannot be driven from application code without a workaround, that workaround is the finding — record it before coding around it.

**Known pre-seeded finding (needs re-verification post-v0.9.12)**: at the time this issue was written, `FakeAsrProvider`/`FakeTtsProvider` lived in `tests/fake_provider.rs` inside the old `agent-runtime-asr-providers`/`agent-runtime-tts-providers` crates. Those crates no longer exist — ASR/TTS now live in `orchest-provider-stream` (streaming dialects) and `orchest-provider-http` (one-shot REST), behind the `orchest-provider` registry wall. The only fake test doubles found today are private, non-reusable structs local to a single test file (`FakeChat`/`FakeAsr` in `crates/orchest-provider/tests/selection.rs`) — there is currently no `FakeTtsProvider` equivalent at all. Before starting this issue, re-audit what test doubles exist under the new architecture; the original "not accessible as a dev-dependency" framing and "one-line `pub use` fix" estimate may no longer hold now that fakes are private to individual test files rather than living in a reusable module.
