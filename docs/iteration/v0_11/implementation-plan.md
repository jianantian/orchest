# v0.11 implementation overview: Research Pipeline

> **Status**: Reconciled with the locked runtime and evidence contracts on
> 2026-07-31.
>
> This document defines cross-issue order and invariants. Each issue's
> executable steps live in its own `plan.md`.

## Authoritative Contracts

- [PRD](prd.md)
- [Design decisions](design-decisions.md)
- [Finding/evidence contract](finding-evidence-contract-design.md)
- [Issue 001 spec](issues/001-demo-spec-scaffold/spec.md) and
  [plan](issues/001-demo-spec-scaffold/plan.md)
- [Issue 002 spec](issues/002-worker-agent/spec.md) and
  [plan](issues/002-worker-agent/plan.md)
- [Issue 003 spec](issues/003-supervisor-watcher-contextmode/spec.md) and
  [plan](issues/003-supervisor-watcher-contextmode/plan.md)
- [Issue 004 spec](issues/004-steering-recovery/spec.md) and
  [plan](issues/004-steering-recovery/plan.md)
- [Issue 005 spec](issues/005-seam-gap-analysis/spec.md) and
  [plan](issues/005-seam-gap-analysis/plan.md)

## Delivery Shape

```text
examples/demo/research-pipeline/
├── Cargo.toml
├── README.md
├── findings.json
├── fixtures/
│   └── research/
├── src/
│   ├── main.rs
│   ├── supervisor.rs
│   ├── worker.rs
│   ├── watcher.rs
│   ├── fault.rs
│   ├── events.rs
│   ├── findings.rs
│   ├── model.rs
│   └── bin/
│       └── seam-report.rs
└── tests/
    ├── findings_contract.rs
    ├── worker.rs
    ├── supervisor_watcher.rs
    ├── failure_escalation.rs
    ├── watcher_order.rs
    └── report_render.rs
```

Package name: `research-pipeline-demo`.

Binary names:

- `research-pipeline`
- `seam-report`

## Non-Negotiable Invariants

### Canonical findings

`examples/demo/research-pipeline/findings.json` is the only editable source
for finding identity, evidence, classification, lifecycle, action ownership,
and readiness. Issue 001 creates it, issues 002–004 update it, and issue 005
renders the report.

### Public paths

Use these public imports:

| API | Public path |
|---|---|
| `RunHandle` | `orchest::run::RunHandle` |
| `EventReceiver` | `orchest::run::EventReceiver` |
| `SupervisionStrategy` | `orchest::run::SupervisionStrategy` |
| `WatcherAction` | `orchest::run::WatcherAction` |
| `LlmWatcher` | `orchest::run::llm_watcher::LlmWatcher` |
| `ContextMode` | `orchest::tool::agent_as_tool::ContextMode` |
| repeated-failure hook | `orchest::hook::{Hook, HookAction, RepeatedFailureHookContext}` |
| `RuntimeEvent` | `orchest::events::RuntimeEvent` |

Private `orchest::run::handle::*` and `orchest::run::config::*` paths are not
valid demo imports.

### LlmWatcher builder

The current signature is infallible:

```rust
let watcher = LlmWatcher::builder()
    .model(Arc::clone(&model))
    .build();
```

Do not add `?`. The panic-on-missing-model behavior is a release-blocker
finding. Changing the signature to `Result` is a separate runtime prerequisite
if it is chosen before demo implementation.

### Delegated-worker boundary

`AgentAsTool` starts and consumes its child run internally. Application code
attaches watchers to the supervisor `RunHandle`; forwarded `SubAgentEvent`s
provide nested evidence. Direct child attachment and child-target steering
are attempted and recorded as gaps unless a public runtime seam is added in a
separate prerequisite change.

### Fault and recovery semantics

The controlled chain is:

```text
ToolError(Fatal, Unsafe)
→ repeated_failure_threshold(1)
→ Hook::on_repeated_failure returns HookAction::Abort
→ worker RunFailed
→ failed AgentAsTool result
→ supervisor escalation
```

`SupervisionStrategy::Restart { max_retries: 1 }` is configured, but restart
is not a success criterion. If no `RunRestarted` event is observed, that is
the expected seam finding.

### Live evidence

Credential absence does not erase deterministic evidence. It leaves the live
run `not-run` and readiness `unverified`. Issue 005 must separately decide
whether this blocks v1.0 or is accepted by a named owner.

## Issue Order

### 001 — Contract before runtime code

Create the package, canonical schema, validator skeleton, renderer CLI,
fixtures, checklist, and pre-seeded findings. Later issues may not invent a
parallel note format.

### 002 — Deterministic worker semantics

Implement worker tools, context modes, and the exact fatal-failure termination
policy. Record only executed evidence.

### 003 — Public observation and steering

Delegate through `AgentAsTool`, attach watchers to the supervisor, observe
forwarded nested events, and demonstrate the actual target of all four
steering paths. Preserve the child-handle gap.

### 004 — Failure escalation and ordering

Run the controlled terminal failure, supervisor escalation, restart
observation, multi-watcher ordering, and terminal-event completion gate.

### 005 — Projection and decisions

Finish validation and deterministic rendering, run available verification,
classify findings, bind blockers, record live status, make the separate v1.0
gate decision, and update downstream documents.

## Cross-Issue Gates

An issue is not complete until:

- its spec acceptance criteria are met or explicitly represented as findings;
- its owned `findings.json` records validate;
- planned commands are not represented as executed evidence;
- generated report prose is not hand-edited to add facts;
- public API examples compile against the current signature;
- any unavailable evidence remains explicit.

Issue 005 additionally requires:

- deterministic render followed by byte-for-byte check;
- every finding present in the report;
- no open seam/release blocker without an owner and action;
- live status and v1.0 release-gate decision recorded independently.

## Verification Sequence

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo test -p research-pipeline-demo
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
cargo run -p research-pipeline-demo --bin seam-report -- check \
  --findings examples/demo/research-pipeline/findings.json \
  --report docs/iteration/v0_11/seam-gap-analysis.md
```

The live command is recorded only when actually executed. Its absence is
represented by the canonical `not-run` state, never by a skipped passing test.
