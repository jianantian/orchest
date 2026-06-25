# v0.11 PRD: Supervised Delegation Demo (Demo B)

## Background

v0.10 (Demo A) validates Orchest breadth: runtime capabilities composing correctly in one coherent product. v0.11 (Demo B) validates depth in the one subsystem that matters most before v1.0—the supervised delegation API surface that Multivac M2's avatar will drive.

Multivac M2 is the automated phase of the Multivac product: an avatar agent that delegates research and writing tasks to worker agents, monitors their progress through event streams, injects steering corrections mid-run, and recovers from worker failures. The Orchest APIs that enable this—`LlmWatcher`, `WatcherAction` steering, `ContextMode`, supervisor recovery, multi-watcher FIFO, and completion gate—have been implemented but have never been exercised by a product-shaped application. Any usability problem, naming confusion, missing primitive, or correctness gap in those APIs must be found before v1.0 freezes them.

Demo B is the evidence collection run. It produces a seam gap analysis that decides which API or documentation issues are release blockers for v1.0.

## Product

Build **Research Pipeline**, a two-level supervised delegation demo. A supervisor Orchest agent receives a research question and delegates the research task to a worker Orchest agent. An `LlmWatcher` monitors the worker's event stream in real time, injects one correction mid-run, and triggers supervisor recovery when the worker fails a controlled fault injection. The supervisor synthesizes the worker's output and writes a final brief.

The demo is intentionally technical rather than product-polished. It must feel like a complete runnable program with a clear output, not a code snippet. Product polish is secondary to API surface coverage.

The demo reuses the research-brief domain and fixture corpus from Briefing Desk where practical, keeping the two demos visually connected in the validation narrative.

## Goals

1. Exercise the full supervised delegation API surface from application code using public Orchest APIs only.
2. Discover naming confusion, missing primitives, unsafe edge cases and documentation gaps in LlmWatcher, Steering, ContextMode, supervisor recovery and completion gate.
3. Produce a seam gap analysis that classifies each finding as a release blocker, post-1.0 backlog or already resolved.
4. Confirm that the Orchest APIs Multivac M2 depends on are stable enough to freeze in v1.0.

## Non-Goals

- Do not build a product-polished CLI; correctness and API coverage matter, UX polish does not.
- Do not use Claude Code as the worker agent; the worker is a plain Orchest agent. Claude-Code-as-tool supervised delegation is a future Multivac M2 validation scenario, not a v0.11 requirement.
- Do not validate the full Multivac RuntimeBackend or v0 daemon stack; those are Multivac product concerns.
- Do not introduce new public runtime concepts for the demo.
- Do not block on v1.0 publishing mechanics; those remain in v1.0 scope.

## Scope

### Demo App

Create a self-contained demo app under:

```text
examples/demo/research-pipeline/
├── Cargo.toml
├── README.md
├── fixtures/
│   └── research/          (symlink or copy from briefing-desk/fixtures/research/)
├── src/
│   ├── main.rs
│   ├── supervisor.rs      (supervisor agent + delegation orchestration)
│   ├── worker.rs          (worker agent + tool set)
│   ├── watcher.rs         (LlmWatcher impl + WatcherAction steering scenarios)
│   ├── fault.rs           (controlled fault injection for recovery test)
│   └── events.rs          (event rendering shared with both agents)
└── tests/
    └── smoke.rs
```

The app may use workspace path dependencies and may import tool definitions from the Briefing Desk crate if they are exported through a public interface. It must not reach into private modules or test-only helpers.

### Required User Flow

1. User runs the demo with a research question.
2. Supervisor agent delegates the task to the worker Orchest agent.
3. `LlmWatcher` attaches to the worker's event stream; both worker and watcher events render to stdout.
4. Watcher returns `WatcherAction::Inject(message)` from `on_event()` to inject one steering command mid-run (a correction, clarification or redirect).
5. Worker processes the injection and continues; supervisor receives the result.
6. Fault injection forces a controlled worker failure; supervisor detects failure and recovers (restart or escalate).
7. Supervisor synthesizes the worker result into a final output.
8. Demo exits with a seam gap analysis report printed to stdout.

A `--fake-model` mode exercises the full path using deterministic fake responses without network access.

### Runtime Capabilities Under Test

| Capability | Demo expectation |
|------------|------------------|
| `LlmWatcher` attach/detach | `RunHandle::attach_watcher()` wires the watcher before delegation; detach is implicit on run completion |
| Worker event visibility | Tool calls, model turns, approval events and status transitions arrive at the watcher via `Watcher::on_event()` |
| Steering via `WatcherAction` | Watcher returns `WatcherAction::Inject(msg)` from `on_event()`; worker processes the injection without panicking or losing state |
| `RunHandle` steering | `RunHandle::inject_message()` / `RunHandle::steer()` cover the external-caller steering path |
| `ContextMode::Fresh` | Worker `SubAgentBuilder::context_mode(ContextMode::Fresh)`: no parent message history in worker context |
| `ContextMode::Fork` | `SubAgentBuilder::context_mode(ContextMode::Fork { depth })`: inherits at most `depth` parent messages; no-messages case errors rather than silently falling back |
| Supervisor recovery | `SupervisionStrategy::Restart { max_retries }` on `AgentConfigBuilder`; observable via `RuntimeEvent::RunRestarted` / `RunAborted` |
| Multi-watcher FIFO | Two `RunHandle::attach_watcher()` calls; event delivery order is deterministic across both watchers |
| Completion gate | Supervisor polls `EventReceiver` for `RuntimeEvent::RunCompleted` / `RunFailed` / `RunAborted`—no fixed timeout |
| Import-path ergonomics | `LlmWatcher` (`run::llm_watcher`) and `ContextMode` (`tool::agent_as_tool`) are not re-exported from lib.rs; ergonomics classified during demo |
| Documentation coverage | README and inline rustdoc are enough to reconstruct the full delegation flow from first principles |

### Fault Injection Scenario

The worker tool set includes a `fault_trigger` tool that returns a structured `ToolError` with `RetryHint::Unsafe` when called. The supervisor or watcher scenario calls this tool once, causing the worker's run to terminate with a failure. The supervisor then demonstrates the recovery path.

This is the only fault injection scenario required. Additional fault shapes (transient, ambiguous) are optional and may be added if they expose API gaps.

### ContextMode Coverage

The demo must exercise both `ContextMode::Fresh` and `ContextMode::Fork { depth }` in two separate code paths (or two separate test cases). Each must:

- Produce the expected behavior (fresh = no inherited messages; fork = at most `depth` messages inherited).
- Fail with a clear error if fork is requested but no messages are available to inherit, rather than silently falling back to fresh.

### Seam API Surface

The v0.11 demo must use the following Orchest public API entry points directly. Any entry point that is missing, misnamed, undocumented, or requires workarounds to use correctly is a seam gap finding.

Accurate public paths (confirmed against codebase before demo is written):

| Seam API | Public type / method | Full path |
|----------|---------------------|-----------|
| Watcher construction | `LlmWatcher::builder()` | `agent_runtime_core::run::llm_watcher::LlmWatcher` |
| Watcher attachment | `RunHandle::attach_watcher(watcher, capacity)` | `agent_runtime_core::run::handle::RunHandle` |
| Steering from watcher | `WatcherAction::Inject(String)` / `WatcherAction::Steer(String)` returned from `on_event()` | `agent_runtime_core::run::watcher::WatcherAction` |
| Steering from external caller | `RunHandle::inject_message(msg)` / `RunHandle::steer(msg)` | `agent_runtime_core::run::handle::RunHandle` |
| Context mode | `SubAgentBuilder::context_mode(ContextMode::Fresh \| Fork { depth })` | `agent_runtime_core::tool::agent_as_tool::ContextMode` |
| Supervisor strategy | `AgentConfigBuilder::supervision_strategy(SupervisionStrategy::Restart { max_retries })` | `agent_runtime_core::run::config::SupervisionStrategy` |
| Failure observation | `RuntimeEvent::RunRestarted { attempt }` / `RunAborted { reason }` | `agent_runtime_core::events::RuntimeEvent` |
| Completion gate | `RuntimeEvent::RunCompleted { output }` / `RunFailed { error }` via `EventReceiver` | `agent_runtime_core::run::handle::EventReceiver` |

**Note**: `InjectCmd` and `SteerCmd` are `pub(crate)` internal types. Do not use them directly; use `WatcherAction` and `RunHandle` methods above.

### Pre-Seeded Findings

These friction points are already known before the demo is written. The demo confirms their impact and produces a final classification. They are not fixed in advance; the demo may reveal they are harmless, or it may confirm they are seam blockers.

| ID | Finding | Preliminary classification |
|----|---------|---------------------------|
| PSF-1 | `LlmWatcher` is not re-exported from `lib.rs`; import path is `agent_runtime_core::run::llm_watcher::LlmWatcher` | Likely post-1.0 (path friction, not a correctness issue) unless Multivac M2 onboarding proves it is confusing |
| PSF-2 | `ContextMode` is not re-exported from `lib.rs`; import path is `agent_runtime_core::tool::agent_as_tool::ContextMode` | Same as PSF-1 |
| PSF-3 | Fake ASR/TTS providers live in `tests/fake_provider.rs` inside each provider crate; they are not accessible as normal dev-dependencies from an external crate. The demo must either vendor the struct or the crates must expose fakes through a `#[cfg(feature = "test-utils")]` feature gate | Likely seam blocker if the demo cannot easily construct fake providers for offline smoke; record workaround used |

## Validation Triage Rule

Findings from Demo B enter one of three buckets. The classification criteria differ from Demo A to reflect the Multivac M2 use case:

1. **Seam blocker**: prevents Multivac M2 from reliably using this API. Fix before v1.0, because v1.0 freezes the public API.
2. **Release blocker**: correctness or safety issue discovered independently of Multivac M2 use. Fix before v1.0.
3. **Post-1.0 backlog**: ergonomic improvement, naming preference, or optional extension. Does not block v1.0 or Multivac M2.

Examples:

- `WatcherAction::Inject` delivery order is non-deterministic when two watchers inject concurrently → seam blocker (Multivac M2 supervisor and avatar may both inject).
- `ContextMode::Fork` silently falls back to `Fresh` instead of failing → seam blocker (`ContextMode` semantics must be explicit before 1.0 freezes them).
- Supervisor recovery requires reading private source to understand which `SupervisionStrategy` variant to set → seam blocker (documentation gap).
- Completion gate works but the event variant name is confusing → post-1.0 if the Multivac M2 team can work with it; release blocker only if renaming before 1.0 is the lesser cost.
- `LlmWatcher` event payload contains more fields than documented → post-1.0.

## Issue Breakdown

| Issue | Title | Scope |
|-------|-------|-------|
| 001 | Demo spec and scaffold | Lock the delegation flow, fixture re-use plan, fake-model contract and seam API checklist |
| 002 | Worker agent and tool set | Implement worker agent with research tools and fault_trigger; fake-model smoke path |
| 003 | Supervisor + LlmWatcher + ContextMode | Implement supervisor delegation, watcher attach/detach, ContextMode::Fresh and Fork paths |
| 004 | Steering injection and supervisor recovery | Implement WatcherAction::Inject scenario, multi-watcher FIFO test, fault injection, supervisor recovery |
| 005 | Seam gap analysis and release-blocker triage | Run full demo, document all seam gaps, classify as seam blocker / release blocker / post-1.0, update v1.0 scope |

## Acceptance Criteria

- [ ] `examples/demo/research-pipeline` exists and builds with workspace path dependencies.
- [ ] Fake-model smoke test covers supervisor delegation, watcher attach, `WatcherAction::Inject` steering, `ContextMode` both variants, fault injection and recovery.
- [ ] Live run works when provider environment variables are configured.
- [ ] `LlmWatcher` attach (`RunHandle::attach_watcher`) produces a visible event stream from the worker on stdout.
- [ ] Steering injection via `WatcherAction::Inject` is processed by the worker and visible in events.
- [ ] `ContextMode::Fresh` and `ContextMode::Fork` both exercise their respective paths; Fork failure produces a clear error.
- [ ] Fault injection causes a controlled worker failure; supervisor recovery path runs without panicking.
- [ ] Two watchers attached concurrently produce deterministic event ordering.
- [ ] Supervisor knows worker is done without relying on timeout.
- [ ] Demo uses only public Orchest APIs.
- [ ] Seam gap analysis report is written and all findings are classified.
- [ ] v1.0 scope is updated from the seam gap analysis.

## Dependencies

- v0.10 complete (validation report may contain supervised delegation friction items that v0.11 inherits; Briefing Desk tool crate may be reused).
- v0.9.5 Control-Flow Hardening (`ContextMode`, handoff snapshot-then-swap, `run_one_step` decomposition).
- v0.9.4 Failure Semantics (`RetryHint`, structured tool failure, `ErrorKind` taxonomy).
- v0.9 Supervised Delegation runtime APIs (LlmWatcher, Steering, supervisor recovery, multi-watcher FIFO).

## Verification

Required local checks:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo test -p research-pipeline-demo
```

The live provider run is manual and env-var gated. It must be documented in the seam gap analysis with exact command, provider, model, date and outcome.

The multi-watcher FIFO test must run in `--fake-model` mode and must be deterministic across repeated runs on the same machine.
