# v0.9.11 PRD: Omni Realtime Provider Evidence

## Background

`docs/todo/provider-unification.md` records the next provider-architecture direction after v0.9.10 Minimax: first collect real omni-provider evidence, then use that evidence to justify provider-core extraction and crate reshaping. Current provider crates are still split by modality (`llm`, `aigc`, `asr`, `tts`), but omni models combine audio input, text reasoning, tool use and audio/text output in a single realtime session. That shape does not fit cleanly in any one existing modality crate.

v0.9.11 is the evidence-gathering feature iteration for that gap. It should connect at least one real omni/realtime provider path and document the runtime shape without prematurely designing the final unified provider abstraction. The downstream refactor iteration must be based on observed behavior from both v0.9.10 Minimax cross-crate duplication and this omni session work.

## Iteration Positioning

This is a satellite provider iteration, not the v1.0 release gate. It may run independently from v0.10 Demo Product Validation as long as the same files are not modified in parallel worktrees. The only hard output needed before the later provider-unification refactor is an implemented first realtime path plus an evidence report; public API stabilization can remain experimental or feature-gated until the refactor PRD/ADR decides the final provider shape.

## Product / Capability

Add an experimental realtime omni provider path that can run a single full-duplex session with audio input and audio/text output, plus enough event surface to observe interleaved transcript, model text, audio chunks and tool-use intent.

The preferred provider is **Doubao / Volcengine realtime** because vendor notes already exist under `docs/external/volceengine/realtime.md`. A Qwen omni provider may be used instead only after adding the relevant vendor reference under `docs/external/` and recording why it is the better first target. A second provider is optional: include it only if it materially improves the abstraction evidence without delaying the first working realtime session.

## Goals

1. Prove how an end-to-end speech model fits into Orchest without pretending it is an ASR → LLM → TTS pipeline.
2. Exercise full-duplex session mechanics: client audio input, realtime output, barge-in/cancel semantics and session close behavior.
3. Reuse the shared `ContentBlock` modality model for audio/text/image-ready data rather than introducing provider-local modality structs.
4. Capture concrete provider-shape evidence for the later provider unification/refactor PRD.
5. Keep the implementation intentionally narrow and experimental so it does not lock in the final provider-core design.
6. Make the next refactor decision easier by separating observed provider facts from proposed abstractions.

## Non-Goals

- Do not merge the LLM/AIGC/ASR/TTS provider crates in this iteration.
- Do not introduce a god-trait that every provider must implement.
- Do not build a generic provider-core crate yet unless a tiny internal helper is required to avoid unsafe duplication.
- Do not replace existing ASR or TTS provider APIs.
- Do not build a UI, hosted voice product or multi-user realtime service.
- Do not require live provider credentials for unit tests or CI.
- Do not make experimental realtime APIs part of the stable v1.0 public surface in this iteration.

## Success Metrics

- One credential-gated live run can send audio input and receive streamed realtime output.
- Fake-session tests cover lifecycle, event mapping and error/cancel behavior without network credentials.
- The evidence report names at least three concrete refactor inputs for the later provider-unification ADR, or explicitly states that fewer were observed.
- No existing ASR, TTS, LLM or AIGC public provider API is broken by the experiment.

## Scope

### Provider Target

Implement at least one provider-backed realtime session path, preferably Volcengine/Doubao realtime, with feature-gated dependencies if new websocket/signing/audio transport dependencies are needed.

The iteration must document:

- authentication and endpoint selection;
- session start/update/close lifecycle;
- inbound audio chunk format and timing assumptions;
- outbound event taxonomy;
- interruption/barge-in behavior when supported by the provider;
- whether tool use is native, emulated or not supported by the selected provider.

### Runtime Shape Under Observation

The implementation should expose enough shape to observe the provider as a capability set rather than a modality bucket:

| Axis | Expected evidence |
|------|-------------------|
| Input content | Audio chunks plus optional text metadata use shared model-layer content concepts where practical |
| Output content | Audio chunks, text deltas/transcripts and provider lifecycle events are distinguishable |
| Interaction primitive | Realtime duplex session is modeled separately from one-shot ASR, turn-based LLM and async generation tasks |
| Tool interaction | Native tool-use events are surfaced if available; otherwise the limitation is explicitly recorded |
| Cancellation | Barge-in/cancel/close semantics are tested or documented as unsupported |

### Required Artifacts

- Issue specs and plans under `docs/iteration/v0_9_11/issues/` before implementation begins.
- A manual-run README or iteration note explaining exact environment variables, input fixture expectations and known provider limitations.
- An evidence report, preferably `docs/iteration/v0_9_11/evidence.md`, separating observed facts, provider-specific quirks and refactor recommendations.

Update `docs/todo/provider-unification.md` only if this iteration discovers new evidence that changes Step 2 assumptions. Do not convert the todo into the refactor design; that belongs to the later provider-unification PRD/ADR.

## Validation Triage Rule

Findings from this iteration must be classified before the next provider refactor:

1. **Omni blocker**: prevents one realtime omni session from working at all. Fix in v0.9.11.
2. **Refactor input**: important evidence for provider-core/unification but not required to make the first omni path work. Record for the later refactor PRD/ADR.
3. **Provider-specific limitation**: vendor behavior that should not shape Orchest's universal abstraction by itself.

Examples:

- Duplicate Volcengine HTTP/signing helpers across provider crates are refactor input, not a reason to merge crates in v0.9.11.
- Missing native tool-use support in the selected realtime provider is a provider-specific limitation unless another omni provider proves the opposite.
- A required duplex event type that cannot be represented without provider-local escape hatches is an omni blocker or refactor input depending on whether it blocks the first demo session.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Protocol decision, vendor notes and fixtures | Confirm selected provider, document protocol shape, add audio/text fixtures and manual-run contract |
| 002 | Duplex session scaffolding | Add feature-gated client/session lifecycle, auth config and deterministic fake session tests |
| 003 | Streaming event mapping | Map provider inbound/outbound events into shared content/event structures without final unification abstractions |
| 004 | Barge-in, close and error semantics | Implement or document cancel/interruption/close behavior and provider error mapping |
| 005 | Omni evidence report | Run manual validation when credentials are available and record refactor inputs for provider unification |

Dependency order: 001 must complete before implementation issues begin; 002 creates the lifecycle surface that 003 and 004 extend; 005 closes the iteration after 002-004 validation outputs exist.

## Acceptance Criteria

- [ ] A v0.9.11 issue set exists under `docs/iteration/v0_9_11/issues/` before implementation begins.
- [ ] One selected realtime/omni provider is documented with vendor source references and credential requirements; a second provider is explicitly accepted or deferred.
- [ ] A feature-gated provider path can start a realtime session, send audio input and receive streamed output events.
- [ ] Output distinguishes audio chunks, text/transcript deltas, lifecycle events and errors.
- [ ] Barge-in/cancel/close behavior is implemented when supported or explicitly documented when unsupported.
- [ ] Unit tests use fake sessions and require no network credentials.
- [ ] Manual live validation is documented with exact command, provider, model, date and outcome.
- [ ] The implementation does not merge provider crates or introduce a final unified provider abstraction.
- [ ] Evidence for the later provider-unification refactor is captured in `docs/iteration/v0_9_11/evidence.md`, an ADR draft or `docs/todo/provider-unification.md` update.

## Dependencies

- v0.9.10 Minimax multimodal provider integration completed, especially the shared multimodal `ContentBlock` groundwork.
- Existing ASR/TTS duplex streaming implementation for lifecycle and test-shape reference.
- `docs/external/volceengine/realtime.md` for the preferred provider target, or newly added official vendor documentation for an alternate omni target.

## Verification

Required local checks:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

Provider live validation is manual and credential-gated. The validation note must include exact command, environment variables used, provider/model, date, whether audio input/output worked, and whether interruption/tool-use behavior was observed or unavailable.
