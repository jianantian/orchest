# v0.11 Demo Implementation Plan: Research Pipeline

> **Reference**: Design decisions are in
> [`design-decisions.md`](./design-decisions.md). This document maps those
> decisions to concrete implementation steps per issue.
>
> **Status**: Drafted from grilling session 2026-07-08.

## Crate Scaffold

```
examples/demo/research-pipeline/
├── Cargo.toml
├── README.md
├── FINDINGS.md                    # raw seam gap findings (issues 002-004)
├── fixtures/
│   └── research/                  # symlink -> ../../briefing-desk/fixtures/research/
├── src/
│   ├── main.rs                    # CLI: single `run` command with flags
│   ├── supervisor.rs              # supervisor agent + delegation orchestration
│   ├── worker.rs                  # worker agent + tool set
│   ├── watcher.rs                 # LlmWatcher impl + CountingWatcher for FIFO
│   ├── fault.rs                   # fault_trigger tool + RepeatedFailureHook
│   ├── events.rs                  # render_event for stdout (unwraps SubAgentEvent)
│   └── fake_model.rs              # SupervisorFakeModel, WorkerFakeModel, WatcherFakeModel
└── tests/
    ├── helpers/
    │   └── mod.rs                 # shared setup: fake models, tool registry, fixtures
    └── smoke.rs                   # 9 focused test functions
```

## Cargo.toml Dependencies

```toml
[package]
name = "research-pipeline-demo"
version = "0.1.0"
edition = "2021"
publish = false

[[bin]]
name = "research-pipeline"
path = "src/main.rs"

[dependencies]
orchest = { path = "../../../crates/orchest" }
orchest-protocol = { path = "../../../crates/orchest-protocol" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
async-trait = "0.1"
clap = { version = "4", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

Note: no `orchest-provider` dependency needed (no ASR/TTS/AIGC - Decision 11).
No `sqlite-session` feature (no session persistence - Decision 16).

## Issue 001: Demo Spec and Scaffold

### Deliverables

1. `Cargo.toml` with workspace path dependencies (see above).
2. `README.md` describing purpose, two-level delegation flow, fake-model
   run command, live provider run command.
3. `fixtures/research/` symlink to Briefing Desk fixtures.
4. `src/main.rs` stub (empty `main` or minimal clap skeleton).
5. Seam API checklist in README or inline.
6. Fake-model contract documented.

### Seam API Checklist

| # | Seam API | Public Path | Status |
|---|----------|-------------|--------|
| 1 | `LlmWatcher::builder()` | `orchest::run::llm_watcher::LlmWatcher` | pending |
| 2 | `RunHandle::attach_watcher(watcher, capacity)` | `orchest::run::handle::RunHandle` | pending |
| 3 | `WatcherAction::Inject(String)` | `orchest::run::watcher::WatcherAction` | pending |
| 4 | `WatcherAction::Steer(String)` | `orchest::run::watcher::WatcherAction` | pending |
| 5 | `RunHandle::inject_message(msg)` | `orchest::run::handle::RunHandle` | pending |
| 6 | `RunHandle::steer(msg)` | `orchest::run::handle::RunHandle` | pending |
| 7 | `SubAgentBuilder::context_mode(ContextMode::Fresh)` | `orchest::tool::agent_as_tool::ContextMode` | pending |
| 8 | `SubAgentBuilder::context_mode(ContextMode::Fork { depth })` | `orchest::tool::agent_as_tool::ContextMode` | pending |
| 9 | `AgentConfigBuilder::supervision_strategy(SupervisionStrategy::Restart { max_retries })` | `orchest::run::config::SupervisionStrategy` | pending |
| 10 | `RuntimeEvent::RunRestarted { attempt }` | `orchest::events::RuntimeEvent` | pending |
| 11 | `RuntimeEvent::RunAborted { reason }` | `orchest::events::RuntimeEvent` | pending |
| 12 | `RuntimeEvent::RunCompleted { output }` via `EventReceiver` | `orchest::run::handle::EventReceiver` | pending |
| 13 | `RuntimeEvent::RunFailed { error }` via `EventReceiver` | `orchest::run::handle::EventReceiver` | pending |

### Fake-Model Contract

**SupervisorFakeModel** (2 steps):
1. Step 1: Emit tool call to delegation tool (`delegate_research`) with the
   research question. Input includes `[FAULT]` marker if `--fault` flag set.
2. Step 2: After receiving tool result, emit final text output (synthesized
   brief or error summary).

**WorkerFakeModel** (3-4 steps):
1. Normal path: `search_corpus` -> `read_file` -> `write_draft` -> end turn.
2. Fault path: `search_corpus` -> `fault_trigger` (if task input contains
   `[FAULT]` marker).
3. After steering injection (if an extra user message appears in history):
   adjust next step (e.g., skip to `write_draft`).

**WatcherFakeModel** (1 evaluation):
1. On first evaluation: return `decide_action(action: "inject", message:
   "Focus on the retention dashboard data")`.
2. Subsequent evaluations: return `decide_action(action: "continue")`.
3. Matches on substrings in formatted events (see Decision 19).

### Acceptance Criteria Mapping

- [ ] `Cargo.toml` exists and compiles (empty binary).
- [ ] `README.md` describes purpose, flow, commands.
- [ ] `fixtures/research/` symlink exists.
- [ ] Seam API checklist added.
- [ ] Fake-model contract documented.
- [ ] No runtime code beyond `main.rs` stub.

## Issue 002: Worker Agent and Tool Set

### Files to Create/Modify

- `src/worker.rs`: Worker agent construction + tool set registration.
- `src/fault.rs`: `FaultTriggerTool` + `AbortOnFatalHook`.
- `src/fake_model.rs`: `WorkerFakeModel`.
- `src/events.rs`: `render_event` (initial version, no SubAgentEvent yet).

### Worker Tool Set

| Tool | Purpose | Metadata |
|------|---------|----------|
| `search_corpus` | Search fixture files by keyword, return ranked paths + snippets | read-only, no approval |
| `read_file` | Read fixture file contents by path | read-only, no approval |
| `write_draft` | Write draft text to output path | `side_effect: true`, approval exercisable |
| `fault_trigger` | Returns `ToolError { kind: Fatal, retry: Unsafe }` with code `FAULT_INJECTED` | no approval |

### FaultTriggerTool Implementation

```rust
async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
    Err(ToolError::fatal("Controlled fault injection for supervisor recovery test")
        .with_code("FAULT_INJECTED")
        .with_retry(RetryHint::Unsafe))
}
```

### AbortOnFatalHook

A `RepeatedFailureHook` that returns `Abort("fault_trigger returned Fatal
error")`. Used with `repeated_failure_threshold(1)` on the worker config
(Decision 3).

### WorkerFakeModel Behavior

Stateless, inspects message history and registered tools:

1. If last user message contains `[FAULT]` marker: emit `search_corpus`
   tool call, then on next step emit `fault_trigger` tool call.
2. If last message is a `ToolResult` from `search_corpus`: emit `read_file`
   call with first search hit path.
3. If last message is a `ToolResult` from `read_file`: emit `write_draft`
   call with synthesized content.
4. If last message is a `ToolResult` from `write_draft`: emit final text
   (end turn).
5. If an extra user message appears in history (steering injection): skip
   to step 4 (emit `write_draft` with adjusted content).

### Acceptance Criteria Mapping

- [ ] Worker agent in `src/worker.rs`, accepts task via `ContextMode::Fresh`.
- [ ] Tool set: `search_corpus`, `read_file`, `write_draft`, `fault_trigger`.
- [ ] `fault_trigger` returns `ToolError(Fatal, Unsafe)`.
- [ ] Worker emits tool-call + model-turn events in fake-model path.
- [ ] Fake-model smoke test for worker passes.
- [ ] Worker is library component, not CLI entry point.

## Issue 003: Supervisor + LlmWatcher + ContextMode

### Files to Create/Modify

- `src/supervisor.rs`: Supervisor agent + delegation orchestration.
- `src/watcher.rs`: `LlmWatcher` construction + `CountingWatcher`.
- `src/fake_model.rs`: Add `SupervisorFakeModel`, `WatcherFakeModel`.
- `src/events.rs`: Add `SubAgentEvent` unwrapping in `render_event`.
- `FINDINGS.md`: Record PSF-1, PSF-2 confirmations.

### Supervisor Agent Construction

```rust
let supervisor_config = AgentConfig::builder("fake/supervisor")
    .system_prompt("You are a research supervisor. Delegate research tasks...")
    .max_steps(5)
    .build()?;

let worker_config = AgentConfig::builder("fake/worker")
    .system_prompt("You are a research worker. Use search_corpus, read_file, write_draft.")
    .max_steps(10)
    .repeated_failure_threshold(1)
    .supervision_strategy(SupervisionStrategy::Restart { max_retries: 1 })
    .build()?;

let delegation_tool = worker_config
    .as_tool("delegate_research", "Delegate a research task to the worker agent")
    .model(Arc::new(WorkerFakeModel))
    .registry(worker_registry)
    .context_mode(ContextMode::Fresh)  // or Fork { depth: 2 }
    .build()?;
```

### LlmWatcher Construction

```rust
let watcher = LlmWatcher::builder()
    .model(Arc::new(WatcherFakeModel))
    .eval_interval(1)
    .system_prompt("Monitor the worker's execution...")
    .build();

handle.attach_watcher(Arc::new(watcher), 1024).await;
```

### ContextMode::Fresh Test

Assert: worker context contains no messages from supervisor history. The
`AgentAsTool::execute()` path with `ContextMode::Fresh` passes
`initial_messages: vec![]` (line 133). Verify via event stream: no
`SubAgentEvent` contains supervisor messages.

### ContextMode::Fork Test

Assert: worker inherits at most `depth` messages. With `Fork { depth: 2 }`
and supervisor history `[system, user, assistant]` (3 messages), worker
inherits last 2 (user + assistant). Verify via event stream or tool result.

### ContextMode::Fork Error Unit Test

Directly construct `ToolContext` with `parent_messages: vec![]`, call
`AgentAsTool::execute()`, assert `ToolError` with code
`EMPTY_PARENT_CONTEXT`.

### Findings to Record

- PSF-1: `LlmWatcher` import path friction. Confirmed post-1.0.
- PSF-2: `ContextMode` import path friction. Confirmed post-1.0.
- P1-3: `ContextMode::Fork` empty-parent-messages error unreachable in
  normal flow.

### Acceptance Criteria Mapping

- [ ] Supervisor in `src/supervisor.rs`, delegates via public API.
- [ ] `LlmWatcher` attached via `RunHandle::attach_watcher()`.
- [ ] Watcher detaches on run completion (implicit).
- [ ] `ContextMode::Fresh` path tested.
- [ ] `ContextMode::Fork { depth }` path tested.
- [ ] Fork with no messages produces clear error (unit test).
- [ ] Fake-model smoke test passes.
- [ ] Findings recorded in `FINDINGS.md`.

## Issue 004: Steering Injection and Supervisor Recovery

### Files to Create/Modify

- `src/watcher.rs`: Add injection logic to watcher, `CountingWatcher`.
- `src/fault.rs`: Wire `AbortOnFatalHook` into worker config.
- `src/supervisor.rs`: Add fault run path, escalation logic.
- `src/main.rs`: Add `--fault` flag, wire both run paths.
- `FINDINGS.md`: Record SB-1 through SB-5, RB-1, P1-4.

### Steering Test: WatcherAction::Inject

1. Attach `LlmWatcher` to supervisor's `RunHandle`.
2. `WatcherFakeModel` returns `Inject("Focus on retention data")` on first
   evaluation.
3. Observe: injection lands in supervisor's conversation (visible in
   supervisor event stream), NOT in worker's.
4. Assert: injected message visible in supervisor events.
5. Assert: injected message NOT visible in worker events (via
   `SubAgentEvent` inspection).
6. Record SB-2.

### Steering Test: WatcherAction::Steer

Same as Inject but returns `Steer("Adjust research focus")`. Same
assertions. Same finding (SB-2).

### Steering Test: RunHandle::inject_message

1. Start supervisor run.
2. Call `handle.inject_message("Manual correction")` after first tool event.
3. Observe: injection lands in supervisor, not worker.
4. Record SB-2 (same finding, different entry point).

### Steering Test: RunHandle::steer

Same as inject_message but calls `handle.steer("System-level steering")`.
Same assertions. Same finding.

### Fault Injection Scenario

1. Run with `--fault` flag. `SupervisorFakeModel` includes `[FAULT]` in
   delegation input.
2. `WorkerFakeModel` calls `search_corpus` then `fault_trigger`.
3. `fault_trigger` returns `ToolError(Fatal, Unsafe)`.
4. `RepeatedFailureHook` returns `Abort` (threshold = 1).
5. `RunFailed` emitted. Worker run terminates cleanly.
6. Supervisor receives `SubAgentEvent { event: RunFailed }` via
   `AgentAsTool` forwarding.
7. Supervisor escalates: writes error summary as final output.

### SupervisionStrategy::Restart Observation

1. Worker config has `SupervisionStrategy::Restart { max_retries: 1 }`.
2. After `RunFailed`, observe: `RunRestarted` is NOT emitted.
3. Record SB-3: restart only triggers on actor crash, not run-level failure.

### Multi-Watcher FIFO Test

1. Attach `LlmWatcher` (watcher 1) and `CountingWatcher` (watcher 2) to
   same `RunHandle`, capacity 1024 each.
2. Run normal flow.
3. Both watchers record event types in order.
4. Assert: same event count for both watchers.
5. Assert: same event type sequence for both watchers.
6. Assert: no `EventsDropped` events observed in event stream.
7. Record SB-5: `try_send` dropping risk for secondary subscribers.

### Completion Gate

1. `main()` polls `EventReceiver` with `while let Some(event) =
   rx.recv().await` until `None`.
2. No timeout. No fixed sleep.
3. Record P1-4: no separate child `EventReceiver`.

### Findings to Record

- SB-1: `AgentAsTool` hides child `RunHandle`.
- SB-2: All four steering paths inject into supervisor, not worker.
- SB-3: `SupervisionStrategy::Restart` doesn't trigger on `RunFailed`.
- SB-4: `LlmWatcher::format_event()` can't format `SubAgentEvent`.
- SB-5: Secondary subscribers use `try_send` (events can drop).
- RB-1: `LlmWatcherBuilder::build()` panics via `expect()`.
- P1-4: No explicit child `EventReceiver`.

### Acceptance Criteria Mapping

- [ ] `WatcherAction::Inject` tested, injection visible in supervisor events.
- [ ] Worker processes injection without panicking (note: lands in supervisor).
- [ ] Two watchers concurrent, deterministic event ordering.
- [ ] Fault injection causes controlled failure.
- [ ] Supervisor detects failure via public API (SubAgentEvent wrapping RunFailed).
- [ ] Supervisor recovery (escalation) runs end-to-end.
- [ ] Completion gate: no fixed timeout.
- [ ] Fake-model smoke test deterministic.
- [ ] All seam gaps recorded in `FINDINGS.md`.

## Issue 005: Seam Gap Analysis and Release-Blocker Triage

### Steps

1. Collect and de-duplicate all findings from `FINDINGS.md`.
2. Classify each as seam blocker / release blocker / post-1.0 backlog.
3. Write report to `docs/iteration/v0_11/seam-gap-analysis.md`.
4. File seam blockers and release blockers as v1.0 issues.
5. Update `docs/iteration/roadmap.md` to mark v0.11 complete.
6. Review Multivac M2 dependency list against findings.

### Report Structure

1. **Executive summary**: whether the supervised delegation API surface is
   ready to freeze in v1.0.
2. **Seam API checklist status**: checklist from issue 001, checked or gap.
3. **Findings table**: ID, API surface, description, workaround,
   classification, action.
4. **Seam blockers detail**: one subsection per SB with repro steps and
   proposed fix.
5. **Release blockers detail**: same structure.
6. **Live run log**: "Not performed" + exact command documented + v1.0
   prerequisite flag.
7. **Multivac M2 readiness verdict**: one sentence per seam API entry.

### Expected Findings (from Design Decisions)

| ID | Classification | Proposed Action |
|----|---------------|-----------------|
| SB-1 | Seam blocker | Expose child RunHandle from AgentAsTool, or provide alternative watcher attachment for delegated runs |
| SB-2 | Seam blocker | Requires SB-1 fix; steering must reach worker conversation |
| SB-3 | Seam blocker | Bridge run-level failure to supervisor recovery (Restart on RunFailed, not just ActorFailed) |
| SB-4 | Seam blocker | Add SubAgentEvent/ChildRunEvent handlers to format_event |
| SB-5 | Seam blocker | Use blocking send for watchers or provide backpressure-aware delivery |
| RB-1 | Release blocker | Change LlmWatcherBuilder::build() to return Result |
| P1-1 | Post-1.0 | Add `pub use run::llm_watcher::LlmWatcher` to lib.rs |
| P1-2 | Post-1.0 | Add `pub use tool::agent_as_tool::ContextMode` to lib.rs |
| P1-3 | Post-1.0 | Document EMPTY_PARENT_CONTEXT as unreachable in normal flow |
| P1-4 | Post-1.0 | Consider child EventReceiver for AgentAsTool |
| P1-5 | Resolved | PSF-3 fixed by hotfix 2026-07-02 |

### Verification Commands

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo test -p research-pipeline-demo
```

### Acceptance Criteria Mapping

- [ ] All findings collected and de-duplicated.
- [ ] Each finding classified.
- [ ] Seam/release blockers filed as v1.0 issues.
- [ ] Report written to `docs/iteration/v0_11/seam-gap-analysis.md`.
- [ ] Live run documented (or explicitly deferred).
- [ ] `docs/iteration/roadmap.md` updated.
- [ ] Multivac M2 dependency list reviewed.
