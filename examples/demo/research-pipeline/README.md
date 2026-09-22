# Research Pipeline

Research Pipeline is the v0.11 evidence-run demo for supervised delegation.
It deliberately tests only Orchest's public APIs and records every seam in
[`findings.json`](findings.json), the sole editable source for finding identity,
evidence, classification, lifecycle, action ownership, and readiness.
Keep prose curated and short: it is an identity/evidence ledger, not an
evaluation dump (no repeated assessment narratives or raw run transcripts).

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

- `orchest::run::RunHandle`, `ChildRunHandle`, `EventReceiver`,
  `SupervisionStrategy`, and `WatcherAction`
- `orchest::run::llm_watcher::LlmWatcher`
- `orchest::tool::agent_as_tool::ContextMode`
- `orchest::hook::{Hook, HookAction, RepeatedFailureHookContext}`
- `orchest::events::RuntimeEvent`

Private `orchest::run::handle::*` and `orchest::run::config::*` paths are out
of bounds. The supervisor owns the root public `RunHandle`. After
`SubAgentStarted`, `RunHandle::child` resolves a public `ChildRunHandle` for
child-target inject/steer and `wait_completion` (SB-1/SB-2/P1-4 verified).
Attached watchers receive supervisor actor-emitted events and, via
`ToolContext::emit_event`, forwarded nested `SubAgentEvent`s (SB-8 verified).
The deterministic watcher action is triggered only by the supervisor-level
delegation `ToolCallStarted { tool: "research_worker", .. }`; nested events are
observed but do not trigger that action.

## Commands

```bash
cargo check -p research-pipeline-demo
cargo test -p research-pipeline-demo --test findings_contract
cargo test -p research-pipeline-demo
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json

# Available now as deterministic CLI shapes; issue 005 owns the final report.
cargo run -p research-pipeline-demo --bin seam-report -- render \
  --findings examples/demo/research-pipeline/findings.json \
  --out docs/review/v0_11_seam_gap_analysis.md
cargo run -p research-pipeline-demo --bin seam-report -- check \
  --findings examples/demo/research-pipeline/findings.json \
  --report docs/review/v0_11_seam_gap_analysis.md
```

`research-pipeline run --question ... --materials fixtures/research` starts
the live provider path. It builds the worker through `SubAgentBuilder`, then
starts the supervisor with `AgentRun::start_with_watchers` so declared watchers
observe from `RunStarted` before the first model call.

## Credential-gated live path

The live provider scenario uses `RESEARCH_PIPELINE_CHAT_MODEL` and its provider
credentials (with optional `RESEARCH_PIPELINE_API_KEY`,
`RESEARCH_PIPELINE_API_URL`, and `RESEARCH_PIPELINE_MAX_TOKENS`). It is not a
CI prerequisite.

```bash
set -a && . ./.env && set +a
export RESEARCH_PIPELINE_CHAT_MODEL=openrouter/anthropic/claude-sonnet-4.6
export RESEARCH_PIPELINE_API_KEY=<openrouter key>

# Normal scenario (run-live-provider)
cargo run -p research-pipeline-demo --bin research-pipeline -- run \
  --question "Is Loom worth continued investment in Q4?" \
  --materials examples/demo/research-pipeline/fixtures/research

# Controlled-fault drill (run-live-provider-controlled-fault). --fault builds the
# worker through Worker::from_paths_fault_drill: the fault instruction lives in
# the worker's own prompt, its tool set is search_corpus + fault_trigger, and the
# live watcher prompt is scoped to the drill, so the run reliably reaches
# run-level Restart and supervisor escalation.
cargo run -p research-pipeline-demo --bin research-pipeline -- run \
  --question "Run the scheduled Q4 investment review over the fixture corpus." \
  --materials examples/demo/research-pipeline/fixtures/research --fault
```

Both scenarios were executed on 2026-09-22 with the repository credentials and
both passed: `run-live-provider` (normal: delegated workers completed, root
`EndTurn`) and `run-live-provider-controlled-fault` (fault → `RunRestarted`
attempt 1 → second fault → escalation without panic) in all four post-repair
runs. The drill's first wiring asked for the fault in the *delegated* request,
which the attached live `LlmWatcher` read as a prompt injection and aborted in
2 of 4 attempts — the reason the fault instruction now sits in the worker's own
prompt and the watcher prompt is drill-scoped. P1-6 records the underlying
observation that the default watcher prompt stated no boundary for `abort` (addressed in #298).

Fixture, deterministic, smoke, and live evidence stay separate: `findings.json`
is the only place that records which of them ran.

The distinct provider smoke is an ignored integration test, so ordinary
`cargo test -p research-pipeline-demo` runs never call a live provider:

```bash
cargo test -p research-pipeline-demo --test smoke \
  credential_gated_provider_path -- --ignored --exact
```

Run that command only after configuring the model and provider credentials.
The test exercises the compiled `research-pipeline run` binary and resolves
the shared fixture symlink before passing the materials path.

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
