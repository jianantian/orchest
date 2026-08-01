# v0.11 PRD: Supervised Delegation Evidence Run (Demo B)

## Background

v0.10 validates Orchest breadth. v0.11 validates the supervised-delegation seams that
Multivac M2 is expected to depend on: delegated execution, event observation, steering,
context transfer, failure escalation, multi-watcher delivery/action ordering,
and completion.

The current public API does not expose a delegated worker's `RunHandle` from
`AgentAsTool`. Consequently, an application can attach a watcher to the supervisor,
observe forwarded `SubAgentEvent`s on the primary supervisor `EventReceiver`, and inject
or steer the supervisor, but it cannot attach directly to or steer the nested worker.
Forwarded child events bypass attached watcher subscription channels because
`ToolContext.event_tx` is the primary subscriber sender. v0.11 must preserve those
facts. The demo is an evidence run: it makes the product-shaped attempt, records what
works, and represents missing seams as stable findings rather than pretending the
desired flow succeeded.

## Product

Build **Research Pipeline**, a two-level supervised delegation demo. A supervisor agent
delegates a research task to a worker through `AgentAsTool`. Application code attaches
watchers to the only public handle it owns—the supervisor `RunHandle`. Those watchers
observe supervisor actor-emitted events; the primary supervisor `EventReceiver`
separately observes forwarded nested-worker events.

The demo authentically attempts the product requirements that the public API cannot yet
satisfy:

- direct watcher attachment to the delegated worker;
- steering that specifically changes the delegated worker;
- restart of the delegated worker after a terminal failure.

Each attempt produces evidence in `findings.json`. A separate renderer validates that
file and generates the human-readable seam-gap report.

The demo is technical rather than product-polished. It must be a complete runnable
program, but its primary output is evidence about the public API contract.

## Goals

1. Exercise supervised delegation using public Orchest APIs only.
2. Separate demonstrated behavior from desired-but-unavailable behavior.
3. Record every seam result through the stable finding/evidence contract.
4. Classify each finding as a seam blocker, release blocker, or post-1.0 backlog item.
5. Produce an explicit v1.0 readiness decision without conflating it with iteration
   completion.

## Non-Goals

- Do not add a runtime seam merely to make the demo's happy path pass.
- Do not reach into `pub(crate)` modules or retain a nested worker handle through test
  hooks.
- Do not use Claude Code as the worker agent.
- Do not validate the Multivac daemon or `RuntimeBackend`.
- Do not build a product-polished CLI.
- Do not make provider credentials a prerequisite for collecting all static and
  deterministic evidence.

## Scope

### Demo App

Create a self-contained demo under:

```text
examples/demo/research-pipeline/
├── Cargo.toml
├── README.md
├── findings.json          # canonical finding/evidence source
├── fixtures/
│   └── research/
├── src/
│   ├── main.rs
│   ├── supervisor.rs
│   ├── worker.rs
│   ├── watcher.rs
│   ├── fault.rs
│   ├── events.rs
│   ├── findings.rs        # typed loading and validation
│   └── bin/
│       └── seam-report.rs # deterministic Markdown renderer
└── tests/
    ├── smoke.rs
    └── findings_contract.rs
```

`findings.json` is the only editable fact source for finding status, classification,
evidence, readiness impact, and follow-up action. Generated Markdown is disposable and
must not contain independently maintained finding facts.

### Required Evidence Flow

1. Start the supervisor and retain its public `RunHandle`.
2. Register two watchers as soon as the handle permits.
   - In deterministic tests, hold the first model call behind a gate, await
     both `attach_watcher()` calls, release a harmless probe step so queued
     subscriptions activate, then require both watcher processors to record
     the second model step before releasing delegation. This proves
     post-registration processing, not capture of every startup event.
   - In the live run, attach immediately after `AgentRun::start`; record that
     public API timing cannot guarantee attachment before the first model
     call or delegation.
3. Delegate to a worker through `AgentAsTool` after both deterministic
   watcher-activation witnesses; do not claim the same ordering guarantee for
   the live path.
4. Observe supervisor actor-emitted events through attached watchers and forwarded
   nested-worker events through the primary supervisor `EventReceiver`. Prove that the
   current attached watcher channels do not receive those forwarded events.
5. Trigger watcher-originated injection specifically from the
   supervisor-level delegation
   `ToolCallStarted { tool: "research_worker", .. }`, never from a child or
   nested event. Attempt external steering and record that both public targets
   are the supervisor, not the nested worker.
6. Exercise `ContextMode::Fresh` and `ContextMode::Fork { depth }`.
7. Trigger a worker tool failure and drive it to terminal failure with
   `repeated_failure_threshold(1)` and an `on_repeated_failure` hook that
   returns `HookAction::Abort`.
8. Observe the worker `RunFailed` result and the supervisor's escalation behavior.
9. Record whether `RunRestarted` occurs. Its absence in the nested-worker path is a seam
   finding, not a successful recovery.
10. Write or update evidence in `findings.json`.
11. Validate `findings.json` and render the seam-gap report.

The live provider path uses `RESEARCH_PIPELINE_CHAT_MODEL`. Without credentials, the
iteration may complete deterministic evidence collection, but live evidence remains
`not-run` and overall readiness remains `unverified`.

### Runtime Capabilities Under Test

| Capability | Evidence expectation |
|---|---|
| Watcher attachment | `RunHandle::attach_watcher()` attaches to the exposed supervisor handle after `AgentRun::start`; deterministic tests gate the first model call, while live evidence records the startup race |
| Nested event visibility | Forwarded `SubAgentEvent`s reach the primary supervisor `EventReceiver`, while attached watcher channels do not receive them |
| Direct worker observation | Make a real application-level attempt; record the missing worker handle as a finding |
| Watcher steering | `WatcherAction::Inject` / `Steer` targets the watched supervisor actor |
| Direct worker steering | Make a real application-level attempt; record that no public target exists |
| External steering | `RunHandle::inject_message()` / `steer()` target the supervisor handle |
| `ContextMode::Fresh` | Worker starts without inherited parent message history |
| `ContextMode::Fork` | Worker inherits at most `depth` parent messages; missing history returns an error |
| Failure termination | Threshold `1` plus `Hook::on_repeated_failure` returning `HookAction::Abort` produces worker `RunFailed` |
| Supervisor escalation | Supervisor observes the failed tool/delegation result and executes its escalation path |
| Restart | Configure `SupervisionStrategy::Restart`; record the absence of nested-worker restart as a finding unless evidence proves otherwise |
| Per-watcher event FIFO | Each watcher observes its own event stream in enqueue order |
| Cross-watcher delivery equivalence | With sufficient capacity and no drop, two watchers observe the same event sequence |
| Cross-watcher action ordering | Independent watcher tasks do not guarantee that `Inject` / `Steer` / `Abort` actions take effect in registration order; record this as a seam gap |
| Completion gate | `EventReceiver` observes a terminal runtime event; no fixed timeout is used as correctness logic |

### Fault and Recovery Contract

`fault_trigger` returns a structured fatal tool error with `RetryHint::Unsafe`. That
error alone does **not** terminate a run and does **not** activate
`SupervisionStrategy::Restart`.

The demo must configure:

```rust
repeated_failure_threshold(1)
Hook::on_repeated_failure(...) -> HookAction::Abort(...)
```

The expected chain is:

```text
fault_trigger returns ToolError(Fatal, Unsafe)
→ repeated failure threshold is reached
→ on_repeated_failure returns HookAction::Abort
→ worker emits RunFailed
→ AgentAsTool returns failure to the supervisor
→ supervisor executes escalation
```

`SupervisionStrategy::Restart` reacts to actor failure in the runtime it supervises. The
delegated worker is started internally by `AgentAsTool`, so a missing
`RuntimeEvent::RunRestarted` in this scenario is expected seam evidence, not proof of a
successful recovery path.

### ContextMode Coverage

The demo exercises both modes in separate deterministic paths:

- `Fresh`: no inherited parent messages.
- `Fork { depth }`: no more than `depth` parent messages; requesting a fork with no
  messages returns a clear error instead of falling back to fresh.

### Public Seam API

Only public paths may be used:

| Seam API | Public path |
|---|---|
| Watcher construction | `orchest::run::llm_watcher::LlmWatcher` |
| Watcher attachment and external steering | `orchest::run::RunHandle` |
| Event receiver | `orchest::run::EventReceiver` |
| Watcher action | `orchest::run::WatcherAction` |
| Context mode | `orchest::tool::agent_as_tool::ContextMode` |
| Supervisor strategy | `orchest::run::SupervisionStrategy` |
| Repeated-failure hook | `orchest::hook::{Hook, HookAction, RepeatedFailureHookContext}` |
| Runtime events | `orchest::events::RuntimeEvent` |

`orchest::run::handle::*` and `orchest::run::config::*` are private implementation
paths. Plans and examples must not import them.

`LlmWatcherBuilder::build()` currently returns `LlmWatcher`, so demo code uses
`.build()`, not `.build()?`. Changing the builder to return `Result` is a separate
runtime change and must be tracked as a blocker before any example adopts the fallible
signature.

## Finding and Evidence Contract

The locked schema and renderer behavior are defined in
[finding-evidence-contract-design.md](finding-evidence-contract-design.md).

Ownership by issue:

- Issue 001 creates `findings.json`, the validator skeleton, and pre-seeded entries.
- Issues 002–004 update their owned findings and evidence records.
- Issue 005 validates the canonical file, renders the report, and records the v1.0
  decision.

No issue creates a parallel free-form finding fact source.

## Validation Triage

Every finding is classified as one of:

1. **Seam blocker**: prevents Multivac M2 from reliably using the API.
2. **Release blocker**: correctness or safety issue independent of Multivac M2.
3. **Post-1.0 backlog**: ergonomic or optional improvement that does not block use.

The classification must be supported by evidence. A desired behavior that cannot be
attempted because the necessary public handle does not exist is still a valid observed
gap when the blocked attempt and inspected public surface are recorded.

## Issue Breakdown

| Issue | Title | Contract |
|---|---|---|
| 001 | Demo contract and scaffold | Create the crate, `findings.json`, validator skeleton, and pre-seeded findings |
| 002 | Worker, context, and fault primitives | Implement worker paths; update owned deterministic evidence |
| 003 | Supervisor observation and steering attempts | Use gated deterministic attachment; record the live startup race, primary-receiver/attached-watcher routing gap, and nested-worker target gaps |
| 004 | Terminal failure, escalation, and ordering | Prove the failure chain and supported event-order properties; record restart and cross-watcher action-order gaps |
| 005 | Evidence validation, report, and release triage | Render exclusively from `findings.json`; record live status and v1.0 decision |

Each issue lives under `issues/<issue-slug>/` and contains both `spec.md` and `plan.md`.
The global implementation plan is an ordering overview only.

## Acceptance Criteria

- [ ] `examples/demo/research-pipeline` builds using workspace path dependencies.
- [ ] `findings.json` validates against the locked finding/evidence contract.
- [ ] A gated deterministic test starts the supervisor, calls
  `attach_watcher()` for both watchers to completion, releases a harmless
  first-step probe, and proves both watcher processors handle the next model
  step before delegation without claiming complete startup capture.
- [ ] Live evidence describes attachment as best-effort immediately after
  start; it does not claim race-free observation from the first event.
- [ ] The absence of a public start-with-watchers or pre-run pause seam is a
  pre-seeded finding when first-event monitoring is required.
- [ ] Direct delegated-worker watcher attachment is authentically attempted and either
  demonstrated or recorded as a gap.
- [ ] Attached watchers observe supervisor actor-emitted events, the primary supervisor
  `EventReceiver` observes forwarded `SubAgentEvent`s, and terminal-complete
  event vectors for both watcher implementations record that forwarded child
  events bypass attached watcher channels.
- [ ] Watcher and external steering target behavior is demonstrated; inability to target
  the delegated worker is recorded as a gap.
- [ ] `Fresh` and `Fork { depth }` behavior is covered deterministically.
- [ ] The fault path uses threshold `1` and an `on_repeated_failure` hook
  returning `HookAction::Abort`, ending in worker `RunFailed` and supervisor
  escalation.
- [ ] Restart is not claimed unless `RunRestarted` evidence exists; otherwise its absence
  is classified as a finding.
- [ ] Each supervisor watcher observes the expected stable supervisor-event milestone
  subsequence for the deterministic scenario without claiming nested-event delivery.
- [ ] With no observed drop, both watchers receive the same event sequence.
- [ ] Cross-watcher action registration order is not claimed; the lack of a
  serialized action-order contract is recorded as a pre-seeded finding.
- [ ] Completion uses terminal events rather than a fixed timeout.
- [ ] All sample imports are public, and `LlmWatcherBuilder::build()` is called with its
  current infallible signature.
- [ ] Issue 005 renders the report from `findings.json`; generated prose adds no facts.
- [ ] If live validation is not run, its evidence is `not-run` and readiness is
  `unverified`.
- [ ] The report separately states whether unverified live evidence blocks v1.0.

## Iteration Completion and v1.0 Readiness

Iteration completion and release readiness are separate decisions:

- v0.11 may complete when all deterministic evidence is collected, all unavailable live
  evidence is explicitly `not-run`, and the canonical findings file validates.
- A `not-run` live scenario forces readiness to `unverified`; it may never be represented
  as ready or passing.
- Issue 005 must make an explicit v1.0 gate decision: either live verification is a
  pre-release action that blocks v1.0, or the remaining uncertainty is accepted by a
  named decision owner with rationale. Closing the iteration does not make that choice
  implicitly.

## Environment Variables

| Variable | Purpose | Required |
|---|---|---|
| `RESEARCH_PIPELINE_CHAT_MODEL` | Provider/model used by supervisor, worker, and watcher | Live only |
| `RESEARCH_PIPELINE_API_KEY` | Provider credential | Live only |
| `RESEARCH_PIPELINE_API_URL` | Optional endpoint override | No |
| `RESEARCH_PIPELINE_MAX_TOKENS` | Optional response-token override | No |

## Verification

Required deterministic checks:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo test -p research-pipeline-demo
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
cargo run -p research-pipeline-demo --bin seam-report -- render \
  --findings examples/demo/research-pipeline/findings.json \
  --out docs/iteration/v0_11/seam-gap-analysis.md
cargo run -p research-pipeline-demo --bin seam-report -- check \
  --findings examples/demo/research-pipeline/findings.json \
  --report docs/iteration/v0_11/seam-gap-analysis.md
```

The live run is manual and credential-gated. Its evidence records the command, provider,
model, date, and outcome. When credentials are absent, the explicit `not-run` record and
`unverified` readiness are the correct iteration result.
