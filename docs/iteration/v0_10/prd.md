# v0.10 PRD: Demo Product Validation

## Background

v1.0 should not be a bucket for every remaining backlog item. Before the first public release, Orchest needs one small but real product built on top of the SDK. The goal is to discover API friction, missing runtime API gaps and documentation gaps through actual usage, then fix only the issues that block product completion or API stability.

This iteration is **Demo A** of a two-demo validation strategy: Demo A (v0.10) validates breadth—runtime capabilities working together in one coherent product. Demo B (v0.11) validates depth—the supervised delegation APIs that the Multivac M2 avatar will depend on. The two demos together form the pre-v1.0 evidence base.

This iteration turns the previously unnamed "SDK validation product" milestone into a concrete pre-1.0 gate.

## Product

Build **Briefing Desk**, a local **multimedia** research-brief agent demo. It takes a directory of mixed research materials — Markdown/text notes, image charts/screenshots, and at least one recorded audio interview — plus a user question. It runs an Orchest agent that transcribes audio sources (ASR), reads image sources through a vision model (multimodal image input), searches and reads text through filesystem tools, streams progress events, asks for approval before writing output, optionally synthesizes an audio version of the brief (TTS), persists the session and produces a final Markdown brief.

The demo is intentionally small in **orchestration** but deliberately broad in **modality**. The modalities are not bolted on for coverage's sake: a real research-briefing tool naturally ingests recorded interviews and chart images and offers an audio version of its output. Breadth falls out of a coherent product, not a checklist.

The demo stays small everywhere else: no web UI, no user accounts, no hosted service, no multi-tenant concerns and no new runtime framework. It must feel like a complete usable tool rather than a code snippet.

## Goals

1. Validate Orchest as a low-level SDK by building a complete app against public APIs only.
2. Exercise the runtime paths that matter before v1.0: model adapter, tool registration, approval, event streaming, session persistence, resume, and one sub-agent or handoff path.
3. **Validate modality breadth**: dogfood the satellite provider gateways (ASR, TTS, multimodal image input) through a real product before v1.0 freezes their public API, so they ship validated rather than untested.
4. Produce a written validation report that decides which API/documentation fixes are release blockers.
5. Keep v1.0 focused on public release mechanics and API stabilization, not broad backlog cleanup.

## Non-Goals

- Do not broaden scope beyond the demo validation flow.
- Do not build a web app, desktop app, hosted service or visual Showroom renderer.
- Do not implement new provider adapters. Use the existing ASR/TTS/AIGC/multimodal gateways as-is; the goal is to validate the public gateway API, not to add providers.
- Do not include AIGC video/music generation, or audio-block-to-LLM input (no provider serializes the `Audio` ContentBlock yet). AIGC image generation is an optional stretch only (see Modality Breadth).
- Do not attempt to validate every provider adapter; one working adapter per modality is enough to dogfood the gateway abstraction. Per-adapter coverage stays in each crate's own tests.
- Do not introduce new public runtime concepts only for the demo.
- Do not publish crates.io/PyPI/npm packages in this iteration.

## Scope

### Demo App

Create a self-contained demo app under:

```text
examples/demo/briefing-desk/
├── Cargo.toml
├── README.md
├── fixtures/
│   ├── research/
│   └── expected/
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── tools.rs
│   ├── media.rs        (ASR transcribe, vision image-in, TTS synthesize; fake providers)
│   ├── events.rs
│   └── validation.rs
└── tests/
    └── smoke.rs
```

The app may use workspace path dependencies. It must not reach into private modules or test-only helpers.

### Required User Flow

1. User runs the demo with a mixed-media materials directory and a question.
2. Agent transcribes audio sources in the directory through an ASR tool.
3. Agent reads image sources (charts/screenshots) through a vision (multimodal image input) model path.
4. Agent searches and reads text materials through filesystem tools.
5. Agent streams visible progress events to stdout across all of the above.
6. Agent asks for approval before writing the final Markdown file.
7. User approval writes the report to an output path.
8. Agent optionally synthesizes an audio version of the brief through a TTS tool (gated by a flag; deny/skip path leaves no audio file).
9. Session state is persisted.
10. A second command resumes from the saved session and appends a follow-up answer.
11. The demo exits with a concise validation summary.

### Modality Breadth

This is the breadth axis the iteration must cover. Each modality is present because the product naturally uses it, not for coverage's sake.

| Gateway / capability | Public surface under test | Natural role in Briefing Desk | Requirement |
|----------------------|---------------------------|-------------------------------|-------------|
| LLM (text) | model adapter | Core reasoning and writing | Required |
| Multimodal image input | `ContentBlock::Image` + vision model adapter | Read a chart/screenshot in the corpus into the brief | Required |
| ASR | `AsrProvider` gateway | Transcribe a recorded interview in the corpus | Required |
| TTS | `TtsProvider` + `VoiceManager` | Synthesize an audio version of the brief | Required |
| AIGC image generation | `agent-runtime-aigc-providers` gateway | Generate one figure/diagram for the brief | Optional stretch — include only if it adds real product value; a forced illustration produces weak validation signal |
| AIGC video/music, audio-block input | — | — | Out of scope |

**Offline discipline.** Every required modality must have a deterministic fake/stub provider so the `--fake` smoke path exercises the full multimedia flow (transcribe → read image → write → synthesize) without network credentials. Live provider runs for each modality are manual and env-var gated, documented in the validation report.

**Why these and not others.** Audio input directly to the LLM is excluded because the `Audio` ContentBlock is a forward placeholder with no provider yet — ASR is the supported bridge from audio to text. AIGC image-out is optional because generating an illustration for a text brief is decoration, not a natural product need; including it on weak grounds would pollute the validation signal. Video and music have no place in a research brief.

### Runtime Capabilities Under Test

| Capability | Demo expectation |
|------------|------------------|
| Model adapter | At least one live provider path, plus deterministic fake-model smoke tests |
| ToolRegistry | Search/read/write/report/transcribe/synthesize tools are registered as normal tools |
| Tool metadata | Write/report/synthesize tools mark side effects and trigger approval |
| Approval | Deny path leaves no output file; approve path writes exactly one report |
| Event stream | CLI renders model/tool/approval/session events without panics across all modalities |
| SessionStore | Session save/resume works across process invocations |
| Sub-agent or handoff | Reviewer sub-agent or handoff validates the draft brief |
| ASR gateway | `AsrProvider` transcribes a corpus audio source; fake provider covers offline smoke |
| Multimodal image input | A corpus image is read through `ContentBlock::Image` into the model; fake vision path covers offline smoke |
| TTS gateway | `TtsProvider` synthesizes an audio brief; fake provider covers offline smoke; skip path leaves no audio file |
| Documentation | README is enough for a new user to run the demo from source |

### Delegation Validation Boundary

v0.10 must validate the lightweight reviewer path, not every delegation shape.

- **Agent-as-Tool** means the parent agent calls a child agent as a normal tool and then continues with the returned result. This is the preferred shape when Briefing Desk needs a reviewer that inspects a draft and returns feedback.
- **Handoff** means the current run-loop control flow transfers to another agent. This is acceptable for the reviewer path only if the demo wants the reviewer agent to take over the session rather than return as a tool result.
- **Supervised long-running delegation** means a delegated worker, such as a Claude-Code-as-tool style agent, is monitored through event streams and can be steered mid-run. This is not a v0.10 requirement. The full supervised delegation API surface—LlmWatcher, Steering, ContextMode, supervisor recovery, multi-watcher FIFO, and completion gate—is validated in Demo B (v0.11).

The v0.10 demo should validate at least one public-API reviewer path using Agent-as-Tool or Handoff. Any friction in `ContextMode`, handoff state, event visibility or resume behavior is recorded in the validation report and classified by the triage rule below. Friction that touches the deeper supervised delegation surface is flagged for v0.11.

## Validation Triage Rule

During v0.10, findings from the demo validation report enter one of three buckets:

1. **Demo blocker**: prevents Briefing Desk from working as specified. Fix in v0.10.
2. **Release blocker**: API stability or correctness issue discovered by the demo. Fix before v1.0.
3. **Post-1.0 backlog**: valuable but not needed for this demo or first public release.

Examples:

- `ContextMode::Fresh | Fork` becomes a release blocker only if the reviewer sub-agent API is confusing or unsafe in the demo.
- `RetryHint` consumption becomes a release blocker only if real tool failures make the demo unreliable or force awkward app-level workarounds.
- `run_one_step` refactor is not a release blocker by itself unless the demo exposes a correctness issue that cannot be fixed locally.
- ASR/TTS/multimodal gateway friction (awkward construction, missing fake provider, confusing event surface, asset-handling rough edges) is now a first-class finding: it becomes a release blocker if the gateway is unusable from application code without workarounds, because v1.0 freezes those public APIs.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Demo product spec and fixtures | Lock the user flow, mixed-media research corpus (text + image + audio), expected outputs and validation rubric |
| 002 | Briefing Desk CLI skeleton | Add app crate, CLI arguments, config loading and deterministic fake-model smoke path |
| 003 | Runtime tool flow | Implement search/read/write/report tools, approval behavior and event rendering |
| 004 | Session resume and reviewer path | Add persisted sessions, resume command and reviewer sub-agent or handoff |
| 005 | Multimedia ingestion and audio output | Wire ASR transcription, multimodal image input and TTS audio output as tools/model paths, each with a fake provider for offline smoke |
| 006 | Validation report and release-blocker triage | Run the demo, document findings (incl. modality gateway friction) and classify follow-up fixes |

## Acceptance Criteria

- [ ] `examples/demo/briefing-desk` exists and builds with workspace path dependencies.
- [ ] Demo README explains setup, fake smoke run, live provider run and resume flow.
- [ ] Fake-model smoke test passes without network credentials and exercises the full multimedia flow (transcribe → read image → write → synthesize).
- [ ] Live run works when provider environment variables are configured.
- [ ] ASR path transcribes a corpus audio source; fake provider covers the offline path.
- [ ] Multimodal image input reads a corpus image through `ContentBlock::Image`; fake vision path covers the offline path.
- [ ] TTS path synthesizes an audio brief; skip flag leaves no audio file; fake provider covers the offline path.
- [ ] Approval deny path is tested and does not write output.
- [ ] Approval approve path writes a deterministic Markdown report shape.
- [ ] Resume flow loads a persisted session and appends a follow-up answer.
- [ ] Demo uses only public Orchest APIs (core + AIGC/ASR/TTS provider crates).
- [ ] Validation report records API friction, modality gateway friction, docs gaps and release-blocker decisions.
- [ ] v1.0 scope is updated from the validation report rather than from unvalidated backlog.

## Path to v0.11

v0.10's validation report feeds directly into v0.11 scope in two ways:

1. **API blockers promoted to v0.11 must-fix**: any release blocker discovered in the reviewer path that touches supervised delegation APIs (LlmWatcher, Steering, ContextMode, supervisor recovery) is handed to v0.11 to confirm the fix under deeper exercise.
2. **Seam gap list**: the validation report produces a named list of supervised delegation friction points. v0.11 begins by running against that list and either closing each item or reclassifying it.

v0.10 does not block on v0.11 scope being defined; it blocks only on v0.10 acceptance criteria being met.

## Dependencies

- v0.9.2 documentation and basic examples.
- v0.7 Agent-as-Tool + Handoff (the reviewer path; supervised delegation / LlmWatcher is **not** a v0.10 dependency — that surface is exercised in v0.11).
- v0.9.5 Control-Flow Hardening (`ContextMode` for the reviewer sub-agent).
- Session persistence from v0.8.
- v0.6.1 Image AIGC Gateway (`agent-runtime-aigc-providers`) — for the optional AIGC stretch.
- v0.9.1 / v0.9.6 ASR Provider Gateway (`agent-runtime-asr-providers`).
- v0.9.3 TTS Provider Gateway (`agent-runtime-tts-providers`).
- v0.9.10 multimodal `ContentBlock` foundation (image input).

## Verification

Required local checks:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo test -p briefing-desk-demo
```

The live provider runs (LLM, ASR, TTS, vision) are manual and env-var gated. Each must be documented in the validation report with exact command, provider, model, date and outcome. The `--fake` smoke path must cover the full multimedia flow with no credentials.
