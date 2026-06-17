# v0.10 PRD: Demo Product Validation

## Background

v1.0 should not be a bucket for every remaining backlog item. Before the first public release, Orchest needs one small but real product built on top of the SDK. The goal is to discover API friction, missing runtime API gaps and documentation gaps through actual usage, then fix only the issues that block product completion or API stability.

This iteration turns the previously unnamed "SDK validation product" milestone into a concrete pre-1.0 gate.

## Product

Build **Briefing Desk**, a local research-brief agent demo. It takes a directory of Markdown/text materials and a user question, runs an Orchest agent, calls filesystem/search/report tools, streams progress events, optionally asks for approval before writing output, persists the session and produces a final Markdown brief.

The demo is intentionally small: no web UI, no user accounts, no hosted service, no multi-tenant concerns and no new runtime framework. It must feel like a complete usable tool rather than a code snippet.

## Goals

1. Validate Orchest as a low-level SDK by building a complete app against public APIs only.
2. Exercise the runtime paths that matter before v1.0: model adapter, tool registration, approval, event streaming, session persistence, resume, and one sub-agent or handoff path.
3. Produce a written validation report that decides which API/documentation fixes are release blockers.
4. Keep v1.0 focused on public release mechanics and API stabilization, not broad backlog cleanup.

## Non-Goals

- Do not broaden scope beyond the demo validation flow.
- Do not build a web app, desktop app, hosted service or visual Showroom renderer.
- Do not add ASR/TTS provider work unless the demo directly needs it.
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
│   ├── events.rs
│   └── validation.rs
└── tests/
    └── smoke.rs
```

The app may use workspace path dependencies. It must not reach into private modules or test-only helpers.

### Required User Flow

1. User runs the demo with a materials directory and a question.
2. Agent searches and reads local materials through registered tools.
3. Agent streams visible progress events to stdout.
4. Agent asks for approval before writing the final Markdown file.
5. User approval writes the report to an output path.
6. Session state is persisted.
7. A second command resumes from the saved session and appends a follow-up answer.
8. The demo exits with a concise validation summary.

### Runtime Capabilities Under Test

| Capability | Demo expectation |
|------------|------------------|
| Model adapter | At least one live provider path, plus deterministic fake-model smoke tests |
| ToolRegistry | Search/read/write/report tools are registered as normal tools |
| Tool metadata | Write/report tools mark side effects and trigger approval |
| Approval | Deny path leaves no output file; approve path writes exactly one report |
| Event stream | CLI renders model/tool/approval/session events without panics |
| SessionStore | Session save/resume works across process invocations |
| Sub-agent or handoff | Reviewer sub-agent or handoff validates the draft brief |
| Documentation | README is enough for a new user to run the demo from source |

### Delegation Validation Boundary

v0.10 must validate the lightweight reviewer path, not every delegation shape.

- **Agent-as-Tool** means the parent agent calls a child agent as a normal tool and then continues with the returned result. This is the preferred shape when Briefing Desk needs a reviewer that inspects a draft and returns feedback.
- **Handoff** means the current run-loop control flow transfers to another agent. This is acceptable for the reviewer path only if the demo wants the reviewer agent to take over the session rather than return as a tool result.
- **Supervised long-running delegation** means a delegated worker, such as a Claude-Code-as-tool style agent, is monitored through event streams and can be steered mid-run. This remains a future validation scenario for long-running agent-tool products; it is not a v0.10 requirement.

The v0.10 demo should validate at least one public-API reviewer path using Agent-as-Tool or Handoff. Any friction in `ContextMode`, handoff state, event visibility or resume behavior is recorded in the validation report and classified by the triage rule below.

## Validation Triage Rule

During v0.10, findings from the demo validation report enter one of three buckets:

1. **Demo blocker**: prevents Briefing Desk from working as specified. Fix in v0.10.
2. **Release blocker**: API stability or correctness issue discovered by the demo. Fix before v1.0.
3. **Post-1.0 backlog**: valuable but not needed for this demo or first public release.

Examples:

- `ContextMode::Fresh | Fork` becomes a release blocker only if the reviewer sub-agent API is confusing or unsafe in the demo.
- `RetryHint` consumption becomes a release blocker only if real tool failures make the demo unreliable or force awkward app-level workarounds.
- `run_one_step` refactor is not a release blocker by itself unless the demo exposes a correctness issue that cannot be fixed locally.
- ASR/TTS provider work remains outside v0.10 unless the demo scope changes to voice input/output.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Demo product spec and fixtures | Lock the user flow, sample research corpus, expected outputs and validation rubric |
| 002 | Briefing Desk CLI skeleton | Add app crate, CLI arguments, config loading and deterministic fake-model smoke path |
| 003 | Runtime tool flow | Implement search/read/write/report tools, approval behavior and event rendering |
| 004 | Session resume and reviewer path | Add persisted sessions, resume command and reviewer sub-agent or handoff |
| 005 | Validation report and release-blocker triage | Run the demo, document findings and classify follow-up fixes |

## Acceptance Criteria

- [ ] `examples/demo/briefing-desk` exists and builds with workspace path dependencies.
- [ ] Demo README explains setup, fake smoke run, live provider run and resume flow.
- [ ] Fake-model smoke test passes without network credentials.
- [ ] Live run works when provider environment variables are configured.
- [ ] Approval deny path is tested and does not write output.
- [ ] Approval approve path writes a deterministic Markdown report shape.
- [ ] Resume flow loads a persisted session and appends a follow-up answer.
- [ ] Demo uses only public Orchest APIs.
- [ ] Validation report records API friction, docs gaps and release-blocker decisions.
- [ ] v1.0 scope is updated from the validation report rather than from unvalidated backlog.

## Dependencies

- v0.9.2 documentation and basic examples.
- v0.9 Supervised Delegation runtime APIs.
- Existing session persistence support from v0.8.

## Verification

Required local checks:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo test -p briefing-desk-demo
```

The live provider run is manual and env-var gated. It must be documented in the validation report with exact command, provider, model, date and outcome.
