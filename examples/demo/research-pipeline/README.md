# Research Pipeline

Research Pipeline is the v0.11 evidence-run demo for supervised delegation.
It deliberately tests only Orchest's public APIs and records every seam in
[`findings.json`](findings.json), the sole editable source for finding identity,
evidence, classification, lifecycle, action ownership, and readiness.

## Evidence flow

```text
findings.json -> seam-report validate -> seam-report render -> seam-gap-analysis.md -> seam-report check
```

`seam-report validate` is read-only. `render` is the explicit deterministic
write path; `check` renders in memory and reports a stale Markdown projection
without writing it. Planned work is never recorded as passed evidence.

The `fixtures/research` symlink points to
`../../briefing-desk/fixtures/research`, the synthetic Briefing Desk corpus.
This keeps the demos on the same source corpus without copying it; it includes
at least the retention dashboard, support-ticket summary, and competitor scan
fixtures (along with the rest of the Briefing Desk corpus).

## Public API boundary

Later issues use these public imports only:

- `orchest::run::RunHandle`, `EventReceiver`, `SupervisionStrategy`, and
  `WatcherAction`
- `orchest::run::llm_watcher::LlmWatcher`
- `orchest::tool::agent_as_tool::ContextMode`
- `orchest::hook::{Hook, HookAction, RepeatedFailureHookContext}`
- `orchest::events::RuntimeEvent`

Private `orchest::run::handle::*` and `orchest::run::config::*` paths are out
of bounds. The supervisor owns the only public `RunHandle`. Attached watchers
receive supervisor actor-emitted events, while forwarded `SubAgentEvent`s
reach the primary supervisor `EventReceiver` and bypass those watcher
subscription channels (SB-8). The deterministic watcher action is triggered
only by the supervisor-level delegation
`ToolCallStarted { tool: "research_worker", .. }`; no child or nested event
triggers it.

## Commands

```bash
cargo check -p research-pipeline-demo
cargo test -p research-pipeline-demo --test findings_contract
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json

# Available now as deterministic CLI shapes; issue 005 owns the final report.
cargo run -p research-pipeline-demo --bin seam-report -- render \
  --findings examples/demo/research-pipeline/findings.json \
  --out docs/iteration/v0_11/seam-gap-analysis.md
cargo run -p research-pipeline-demo --bin seam-report -- check \
  --findings examples/demo/research-pipeline/findings.json \
  --report docs/iteration/v0_11/seam-gap-analysis.md
```

`research-pipeline run --question ... --materials fixtures/research` starts
the live provider path. It builds the worker through `SubAgentBuilder`, starts
the supervisor, then immediately attaches both watchers. This is explicitly
best-effort: the public API cannot guarantee attachment before delegation or
the first event.

## Credential-gated live path

The live provider scenario uses `RESEARCH_PIPELINE_CHAT_MODEL` and its provider
credentials (with optional `RESEARCH_PIPELINE_API_KEY`,
`RESEARCH_PIPELINE_API_URL`, and `RESEARCH_PIPELINE_MAX_TOKENS`). It is not a
CI prerequisite. Until a real command is executed and recorded, the required
live-provider run remains `not-run`, and the contract requires the readiness
verdict to remain `unverified`.

## Updating evidence

Issues 002–004 update the same `findings.json`: they add executed run and
evidence records, then reference those records from the already seeded
checklist items and findings. Reproductions append evidence to the original
id rather than minting a duplicate. A repair is at most `implemented` until
its declared verifier passes; no issue removes negative findings to improve
the report. Issue 005 performs final triage and renders the checked-in report.

Source evidence locates a repository-relative POSIX `path` plus a stable
`symbol`; mutable line numbers are never canonical locators. Do not put
credentials, prompts, raw provider payloads, machine-local paths, or full tool
input/output in the evidence file.
