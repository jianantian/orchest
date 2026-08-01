# v0.11 Design Decisions: Supervised Delegation Demo (Demo B)

> **Status**: Locked through grilling session 2026-07-08; evidence-contract
> amendments locked 2026-07-31.
> This document records every design decision and pre-identified seam gap
> finding before implementation begins. Issues 001-005 implement against
> these decisions; issue 005 synthesizes the findings into the final seam
> gap analysis report.

## How This Document Was Produced

A structured grilling session walked down every branch of the v0.11 design
tree, resolving dependencies between decisions one by one. Each section
below corresponds to one resolved question. For each: the problem, the
options considered, the decision, and the rationale.

---

## Decision 1: Delegation Architecture - Single Unified Flow

### Problem

The PRD requires both `SubAgentBuilder::context_mode()` (delegation through
`AgentAsTool`) and `RunHandle::attach_watcher()` on the worker. These are
architecturally incompatible in a single flow:

- `AgentAsTool::run_child_attempt()` calls `AgentRun::start_with_bus()`,
  retains the child `RunHandle` internally, and consumes it through
  `RunHandle::wait()` before returning.
- `RunHandle::attach_watcher()` routes registration through
  `SupervisorMsg::RegisterWatcher`. Calling it on the supervisor's
  `RunHandle` subscribes to the supervisor run's worker actor, not the
  delegated `AgentAsTool` child.

### Decision

**Single unified flow.** The supervisor is an Orchest agent that delegates
through `AgentAsTool`. The demo attempts to use the unified flow and
discovers the resulting seam gaps through authentic attempt.

### Rationale

The alternative (two-path split: direct `AgentRun::start` for watcher
access + `SubAgentBuilder` for ContextMode) would avoid the gaps but hide
them. Demo B's purpose is to discover gaps, not avoid them.

---

## Decision 2: Injection Target Mismatch - Document the Gap

### Problem

When the supervisor delegates via `AgentAsTool`, `WatcherAction::Inject`
returned from `on_event()` injects into the **supervisor's** conversation,
not the worker's. `run::supervisor::reattach_watcher()` maps the returned
action to `AgentMsg::Inject` on the supervisor run's shared worker actor.
The child run's conversation is inaccessible.

### Decision

**Document the gap.** The demo attempts `WatcherAction::Inject` from an
attributable supervisor tool-call event for the `AgentAsTool`. It observes
and documents that the injection lands in the supervisor's conversation, not
the worker's. The primary supervisor `EventReceiver` separately observes the
forwarded `SubAgentEvent`; attached watchers do not receive it (Decision 27).
No workarounds (no vendoring private types, no reaching into `AgentAsTool`
internals).

### Rationale

The gap IS the finding. Working around it would hide exactly what Demo B
exists to discover.

---

## Decision 3: Fault Injection Recovery - RunFailed + Document Restart Gap

### Problem

`SupervisionStrategy::Restart` only triggers in
`SupervisorActor::handle()` on `SupervisionEvent::ActorFailed`, which is a
ractor-level actor crash. A tool returning `ToolError(Fatal, Unsafe)` does
NOT crash the actor:

1. The run loop feeds the error back to the model as a `ToolResult`.
2. `run::actor::record_repeated_failure()` returns `Ok(())` on the first
   call when the threshold remains at its default of 3.
3. The model continues. No `RunFailed`, no `ActorFailed`, no restart.

The existing actor-restart tests trigger this by panicking inside the model
adapter; see the `RestartInputRecordingModel` test fixture.

### Decision

Use `repeated_failure_threshold(1)` plus a `Hook::on_repeated_failure`
implementation that returns `HookAction::Abort` on fatal tool errors. This
produces `RunFailed` (clean run termination). The supervisor detects the
failure through `SubAgentEvent` wrapping `RunFailed`. The supervisor then
escalates (writes error summary).

`SupervisionStrategy::Restart { max_retries: 1 }` is set on the worker
config. The demo observes that `RunRestarted` is NOT emitted and documents
the gap.

### Rationale

This is the most honest approach. It discovers the real gap between
run-level failure and actor-level crash. The escalation path satisfies the
"supervisor recovery path runs" criterion.

---

## Decision 4: Completion Gate - Application EventReceiver + Watcher Observation

### Problem

In the unified flow, the supervisor agent receives worker completion
implicitly as a tool result from `AgentAsTool::execute()`. There is no
separate `EventReceiver` for the child run.

### Decision

**Both levels:**

1. **Application-level:** `main()` polls the supervisor's `EventReceiver`
   with `while let Some(event) = rx.recv().await` until `None`. No timeout.
2. **Watcher-level:** attached watchers observe supervisor actor-emitted
   events, including the supervisor terminal event. They do not receive
   forwarded `SubAgentEvent`s in the current public flow (Decision 27).

### Rationale

The implicit completion via tool result is usable. An explicit child
`EventReceiver` would be nicer but isn't blocking. Recorded as post-1.0
finding P1-4.

---

## Decision 5: ContextMode::Fork Error Case - Unit Test + Document Unreachability

### Problem

The `ContextMode::Fork` branch in `AgentAsTool::execute()` checks
`ctx.parent_messages.is_empty()`. But `parent_messages =
state.messages.clone()`, and `state.messages` always starts with at least
`[system_prompt, user_input]`. The error is unreachable through normal agent
flow.

### Decision

**Both:**

1. **Demo flow:** Exercise `ContextMode::Fresh` and `ContextMode::Fork {
   depth: 2 }`. Assert Fresh produces no inherited messages; Fork inherits
   the expected count.
2. **Separate unit test:** Directly construct a `ToolContext` with
   `parent_messages: vec![]` and assert the `EMPTY_PARENT_CONTEXT` error.

### Rationale

The unit test proves the error exists. The demo flow proves the normal
paths work. The unreachability is documented as post-1.0 finding P1-3.

## Decision 6: Separate Event FIFO, Delivery Equivalence, and Action Ordering

### Problem

`run::actor::emit()` has asymmetric delivery:

- **Primary subscriber (index 0):** blocking `send()` with timeout.
- **Secondary subscribers (index 1+):** `try_send()` - non-blocking. Events
  are dropped if the channel is full.

Within one watcher channel, accepted events retain enqueue order. With enough
capacity and no drops, two watchers can observe the same event sequence.

However, `run::supervisor::reattach_watcher()` starts one independent Tokio
task per watcher. Each action is applied when that watcher's `on_event()`
future completes. Registration order therefore does not establish a global
order for `Inject`, `Steer`, or `Abort` actions.

### Decision

**Test and document three distinct properties:**

1. **Per-watcher FIFO:** each watcher records stable event keys for a
   deterministic scenario and asserts the expected milestone subsequence
   (for example model call → tool call → tool result → terminal event).
2. **No-drop delivery equivalence:** with capacity 1024 and no observed
   `EventsDropped`, both watchers receive the same sequence.
3. **Cross-watcher action order:** do not infer this from event equality.
   Record the missing registration-order guarantee as seam blocker SB-7.

Continue to record secondary-subscriber `try_send` loss as seam blocker
SB-5.

### Rationale

The deterministic test proves only the first two properties. The source
topology disproves a public registration-order guarantee for the third;
an adversarial gated-watcher reproduction may add evidence but is not needed
to pretend the stronger contract exists.

---

## Decision 7: Model Adapters - Single Chat Model, Three Roles

### Decision

Three model adapters (supervisor, worker, watcher), all backed by the
same chat model adapter constructed from the `RESEARCH_PIPELINE_CHAT_MODEL`
env var via `orchest_provider::create_adapter_from_config`:

| Adapter | Role | Behavior (driven by system prompt + tool set) |
|---------|------|-----------------------------------------------|
| Supervisor adapter | Supervisor agent | Emit delegation tool call -> synthesize final brief |
| Worker adapter | Worker agent | search_corpus -> read_file -> write_draft (normal) or search_corpus -> fault_trigger (fault path) |
| Watcher adapter | LlmWatcher's model | Return `decide_action(action: "inject", message: "correction")` when event buffer contains a tool-call event |

Each adapter is an `Arc::clone(&model)` of the single chat model adapter.
Behavior is determined by each agent's system prompt and tool set, not by
the model adapter itself.

### Rationale

On the live path, `RESEARCH_PIPELINE_CHAT_MODEL` configures the chat model
via `create_adapter_from_config`. No custom model type is used by the live
binary. Each role is distinguished by its system prompt and tool set.
Deterministic tests may use a test-only gated model to establish observable
post-registration behavior; that evidence remains separate from the
credential-gated provider run.

---

## Decision 8: Fault Trigger - Two Separate Runs

### Decision

Two distinct demo runs:

1. **Normal run:** Supervisor delegates -> worker does search/read/write_draft;
   the supervisor-attached watcher injects into the supervisor, the worker
   continues independently, and the target mismatch is recorded before the
   supervisor synthesizes.
2. **Fault run:** Supervisor delegates -> worker does search -> fault_trigger
   -> `RunFailed` -> supervisor detects failure -> escalation.

The fault path is triggered by the `--fault` CLI flag. The supervisor's
system prompt instructs the worker (via the delegation tool call) to call
`fault_trigger` after `search_corpus` instead of proceeding to
`read_file`.

### Rationale

Keeps scenarios clean and deterministic. Combining them adds complexity.

---

## Decision 9: Fixtures - Symlink

### Decision

`examples/demo/research-pipeline/fixtures/research` ->
`../../briefing-desk/fixtures/research` (symlink).

### Rationale

Zero duplication, both demos stay in sync. macOS/Linux handle symlinks
fine. Git tracks symlinks natively. The smoke test resolves the symlink
path (`fs::canonicalize`) before passing to tools.

---

## Decision 10: Tools - Port Fresh

### Decision

Implement `search_corpus`, `read_file`, `write_draft`, `fault_trigger`
directly in the Research Pipeline demo. No sharing from Briefing Desk
(which is a `[[bin]]` crate, not importable anyway).

### Rationale

The tools are trivially simple (10-30 lines each). Different names and
semantics from Briefing Desk. `fault_trigger` is entirely new. Keeping the
demo self-contained ensures any friction found is Orchest's, not
inter-demo dependency friction.

---

## Decision 11: PSF-3 - Moot

### Decision

PSF-3 (fake ASR/TTS providers inaccessible) is **resolved by hotfix
2026-07-02**. `orchest-provider` exposes `fakes::{FakeAsr, FakeTts}` behind
the `testing` feature. The Research Pipeline demo does not use ASR/TTS
(delegation depth, not modality breadth). Mark PSF-3 as resolved.

---

## Decision 12: Live Provider Run - Distinct Evidence Path

### Decision

The `RESEARCH_PIPELINE_CHAT_MODEL` env var configures the live chat model
adapter via `orchest_provider::create_adapter_from_config`. Live execution
is a required evidence row, but credentials are not a prerequisite for
closing deterministic evidence collection. When credentials are absent, the
live run remains `not-run` and readiness is `unverified`.

### Rationale

Fixture, deterministic test, smoke, and live evidence prove different
things. A missing live run must remain visible and cannot be converted into
a pass, but it also should not erase valid static and deterministic seam
evidence. Whether `unverified` readiness blocks v1.0 is a separate release
decision recorded by issue 005.

---

## Decision 13: PSF-1/PSF-2 Import Paths - Post-1.0, Recommend Fix

### Decision

Record as **post-1.0**. Use full import paths in the demo. Recommend adding
`pub use` re-exports to `lib.rs` as a trivial v1.0 polish item. Not a seam
blocker since the APIs work - they're just verbose to import.

### Rationale

The inconsistency (`Watcher` and `WatcherAction` are re-exported from
`lib.rs` but `LlmWatcher` is not) is confusing but not blocking.

---

## Decision 14: Steering Coverage - All Four Paths

### Decision

Exercise all four steering paths, each with its own test:

1. `WatcherAction::Inject(String)` - from watcher, user-role
2. `WatcherAction::Steer(String)` - from watcher, system-role
3. `RunHandle::inject_message(&str)` - from external caller, user-role
4. `RunHandle::steer(&str)` - from external caller, system-role

All four share the same limitation (inject into supervisor, not worker).
The seam gap analysis consolidates the shared finding.

### Rationale

Maximum API coverage. The shared finding is consolidated rather than
repeated four times.

---

## Decision 15: Test Structure - Focused Test Targets

### Decision

Use focused integration-test targets. Provider-independent contracts run
without credentials; the end-to-end smoke path is credential-gated:

| Target | Scenario |
|---|---|
| `findings_contract` | schema, reference, lifecycle, and readiness rules |
| `worker` | tools, Fresh/Fork, fault threshold and abort hook |
| `supervisor_watcher` | delegation, supervisor attachment, nested observation, steering targets |
| `failure_escalation` | fault trigger → `RunFailed` → supervisor escalation |
| `watcher_order` | per-watcher FIFO, no-drop sequence equivalence, action-order gap |
| `report_render` | deterministic render and stale-report detection |
| `smoke` | credential-gated provider path |

The Fork empty-parent-messages error case may be a worker integration test or
a unit test beside the context helper.

### Rationale

Each acceptance criterion maps to a narrow test target. Failures remain
isolated, while unavailable provider credentials do not skip the evidence
contract or deterministic runtime checks.

---

## Decision 16: CLI - Single Run Command with Flags

### Decision

The `research-pipeline` binary has one `run` command with `--question`,
`--fault`, and `--materials`. It has no resume path. Stdout is the event
trace plus final output.

The separate `seam-report` binary validates `findings.json`, explicitly
renders the Markdown report, and checks it for staleness. It never runs the
demo or invents finding facts.

### Rationale

Simpler than Briefing Desk's CLI. Still a runnable program that feels
"complete" per the PRD's requirement.

---

## Decision 17: Supervisor Type - Orchest Agent

### Decision

The supervisor is an Orchest agent with its own model adapter. It delegates
to the worker through `AgentAsTool` (produced by `SubAgentBuilder`).

### Rationale

The PRD says "supervisor Orchest agent." Every seam gap discovered flows
from this choice - that's the point of Demo B.

---

## Decision 18: Watcher Type - LlmWatcher + CountingWatcher

### Decision

**Both:**

1. **`LlmWatcher`** for steering tests. Backed by the same chat model
   adapter as the supervisor and worker. Set `eval_interval(1)`. Returns
   `Inject("correction")` when event buffer contains a tool-call event.
2. **Custom `CountingWatcher`** for per-watcher FIFO and no-drop
   cross-watcher sequence equivalence. Deterministic, no model dependency.

### Rationale

`LlmWatcher` is the PRD-required watcher type and discovers the
`format_event` gap (SB-4). `CountingWatcher` isolates event-delivery
properties without making a claim about cross-watcher action ordering.

---

## Decision 19: format_event Gap - Use As-Is, Let Gap Surface

### Problem

`LlmWatcher::format_event()` has no handler for `SubAgentEvent`,
`SubAgentStarted`, `SubAgentCompleted`, `SubAgentFailed`, or
`ChildRunEvent`. If passed directly, these fall into the catch-all `Debug`
formatter. In the unified public flow, forwarded child events bypass attached
watchers entirely (Decision 27), so this is source evidence rather than a
claim that the demo's `LlmWatcher` received nested events.

### Decision

Use `LlmWatcher` as-is for supervisor actor-emitted events. Preserve the
formatting limitation as source evidence, but do not claim the watcher model
received forwarded nested events.

### Rationale

Working around it would hide the gap.

---

## Decision 20: Event Rendering - Custom render_event for Stdout

### Decision

`events.rs` provides `render_event(event: &RuntimeEvent) -> String` for
stdout - unwraps primary-receiver `SubAgentEvent` values to show `[worker]
Tool completed: search_corpus, 5ms`. The attached `LlmWatcher` receives only
supervisor actor-emitted events in this flow. Its latent nested formatting
gap remains separate source evidence.

### Rationale

Separates "what the user sees" (clean) from "what the watcher's model sees"
(raw debug dumps). The difference is the seam gap evidence.

---

## Decision 21: SupervisionStrategy - Set Restart, Observe Gap

### Decision

Set `SupervisionStrategy::Restart { max_retries: 1 }` on the worker
config. Run the fault scenario. Observe that `RunRestarted` is NOT emitted.
Document the gap. The supervisor falls back to escalation.

### Rationale

Exercises the `SupervisionStrategy::Restart` API (satisfies the seam API
checklist). Discovers the gap between run-level failure and actor-level
crash.

---

## Decision 22: LlmWatcherBuilder::build() Panic - Release Blocker

### Problem

`LlmWatcherBuilder::build()` panics via `expect()` when `.model()` was not
called. This violates the AGENTS.md ban on `expect()` in library code.
`SubAgentBuilder::build()` correctly returns `Result` for the same pattern
(fixed in hotfix 2026-07-02, #199).

### Decision

Record as **release blocker**. `build()` should return
`Result<LlmWatcher, ConfigError>` before v1.0 freezes the API. The demo
calls the current signature correctly as `.build()` (with `.model(...)`),
not `.build()?`, so it won't panic in practice. If the runtime signature is
changed first, that change is an explicit blocker and the demo is updated in
the same change.

### Rationale

Changing `build()`'s signature after v1.0 would be a breaking change. The
fix is straightforward: add a `ConfigError::WatcherMissingModel` variant.

---

## Decision 23: Model Instances - Three Arc Clones

### Decision

Three `Arc::clone(&model)` from the single chat model adapter. One clone
for the supervisor agent, one for the worker agent, one for the
`LlmWatcher`. No separate model types, no shared mutable state.

### Rationale

The live provider path follows the Briefing Desk adapter pattern. Each role
is distinguished by its system prompt and tool set, not by a custom model
implementation. Credential-free contract tests do not change the live model
topology.

---

## Decision 24: Findings Tracking - Canonical JSON -> Generated Report

### Decision

`examples/demo/research-pipeline/findings.json` is the only editable fact
source. Issue 001 creates it; issues 002–004 add evidence and update their
owned findings; issue 005 validates it and deterministically renders
`docs/iteration/v0_11/seam-gap-analysis.md`.

The generated Markdown is a review artifact, not a second source of finding
status, classification, evidence, readiness, or action ownership.

### Rationale

Stable ids, typed evidence references, and deterministic rendering prevent
the implementation notes, final report, and release triage from drifting.
The full contract is defined in
[finding-evidence-contract-design.md](finding-evidence-contract-design.md).

---

## Decision 25: Iteration Completion Is Not Release Readiness

### Decision

v0.11 may close after deterministic evidence collection when unavailable
live evidence is explicitly recorded as `not-run` and the readiness verdict
is `unverified`.

Issue 005 separately records whether that unresolved live verification:

1. blocks v1.0 until executed; or
2. is accepted by a named decision owner with rationale.

Closing v0.11 never implies the second decision.

### Rationale

This preserves honest evidence without making credential availability a
hidden iteration state machine or an implicit release waiver.

---

## Decision 26: Watcher Attachment Has a Startup Race

### Problem

`AgentRun::start()` immediately calls the supervised spawn path and returns a
`RunHandle` only after execution has been scheduled. `RunHandle::attach_watcher()`
must then wait for the supervisor reference and register the watcher. The
public API has no constructor that accepts watchers and no pause-before-first-
model-call seam.

The runtime test `attach_watcher_does_not_duplicate_supervisor_subscription`
uses `GatedMultiStepModel` to prevent the first model call from progressing
until watcher attachment completes. That test technique is not available
against an ordinary live provider.

### Decision

Use two evidence contracts:

1. **Deterministic test:** gate the first model call, start the run, attach
   both watchers, await both `attach_watcher()` calls, then release a harmless
   probe tool step. Once that `RunStep` ends, gate the second model call and
   require both watcher processors to record its `ModelCallStarted` event
   before releasing delegation. This proves active post-registration
   processing; it still does not prove that every startup event was captured.
2. **Live run:** attach immediately after `AgentRun::start()` and describe the
   result as best-effort. Do not claim that the watcher observed the first
   event or was attached before delegation.

Record the missing start-with-watchers / public pre-run pause primitive as
seam blocker SB-6 when monitoring from the first event is a Multivac M2
requirement.

### Rationale

The gate makes the deterministic test meaningful without laundering a test
fixture into a live API guarantee. A product that requires complete
observation from event zero needs a runtime seam, not tighter application
timing.

---

## Decision 27: Forwarded Child Events Bypass Attached Watchers

### Problem

The run actor constructs `ToolContext.event_tx` from `primary(subs).clone()`.
`AgentAsTool::run_child_attempt()` forwards `SubAgentEvent` and child
lifecycle events by sending directly to that sender.
`RunHandle::attach_watcher()` registers a different subscriber channel
through `AgentMsg::Subscribe`. Consequently, forwarded child events reach the
primary supervisor `EventReceiver` but are not broadcast to attached watcher
channels.

### Decision

Prove three separate properties in the gated deterministic scenario:

1. after a harmless probe step activates queued subscriptions, both attached
   watcher processors complete an attributable supervisor
   `ModelCallStarted` before delegation;
2. the primary supervisor `EventReceiver` receives forwarded
   `SubAgentEvent`s; and
3. after both the custom watcher and the `LlmWatcher` wrapper complete
   processing a terminal supervisor event, neither completed event vector
   contains a forwarded child event.

Record the missing broadcast/observation seam as SB-8. Do not change runtime
behavior in the evidence demo and do not use a timeout as negative proof.
The custom watcher's action is attributable specifically to the
supervisor-level delegation
`ToolCallStarted { tool: "research_worker", .. }`; it is never triggered by a
child or nested event.

### Rationale

The product goal requires nested observation by supervisor-attached watchers.
Primary-receiver visibility is useful but is not equivalent to watcher
delivery. Keeping the two channels explicit prevents the demo from claiming a
capability the public runtime does not provide.

---

## Pre-Identified Seam Gap Findings Summary

Findings locked through the grilling session plus SB-8, which the gated
implementation test discovered. These are preliminary classifications - the
demo confirms their impact during implementation and issue 005 produces final
classification in the seam gap analysis report.

### Seam Blockers (prevent Multivac M2 from reliably using the API)

| ID | Finding | Source Decision |
|----|---------|----------------|
| SB-1 | `AgentAsTool` delegation does not expose child `RunHandle` - cannot attach watcher or inject steering into worker when delegation goes through `SubAgentBuilder` | D1, D2 |
| SB-2 | `WatcherAction::Inject`/`Steer` and `RunHandle::inject_message`/`steer` inject into supervisor, not worker, when delegation uses `AgentAsTool` | D2, D14 |
| SB-3 | `SupervisionStrategy::Restart` only triggers on actor-level crash (panic), not on run-level failure (`RunFailed` from tool errors, budget, max_steps) | D3 |
| SB-4 | `LlmWatcher::format_event()` has no handler for `SubAgentEvent`/`SubAgentStarted`/`SubAgentCompleted`/`SubAgentFailed`/`ChildRunEvent` - LLM-powered watcher can't understand nested delegation events | D19 |
| SB-5 | Secondary event subscribers (watchers) use `try_send` - events can be dropped under backpressure with no watcher-side recovery | D6 |
| SB-6 | `AgentRun::start()` begins execution before application code can call `RunHandle::attach_watcher()`; no public start-with-watchers or pre-run pause seam guarantees observation from the first event | D26 |
| SB-7 | Watchers run in independent tasks, so actions returned by multiple watchers have no global registration-order execution guarantee | D6 |
| SB-8 | Forwarded `AgentAsTool` child events are sent only through the primary supervisor `EventReceiver` and bypass attached watcher subscription channels | D27 |

### Release Blockers (correctness/safety issues, independent of M2)

| ID | Finding | Source Decision |
|----|---------|----------------|
| RB-1 | `LlmWatcherBuilder::build()` panics via `expect()` when `.model()` not called - violates AGENTS.md `expect()` ban; should return `Result` before v1.0 freezes API | D22 |

### Post-1.0 Backlog (ergonomics, naming, optional extensions)

| ID | Finding | Source Decision |
|----|---------|----------------|
| P1-1 | `LlmWatcher` not re-exported from `lib.rs` (PSF-1) - import path `orchest::run::llm_watcher::LlmWatcher` | D13 |
| P1-2 | `ContextMode` not re-exported from `lib.rs` (PSF-2) - import path `orchest::tool::agent_as_tool::ContextMode` | D13 |
| P1-3 | `ContextMode::Fork` empty-parent-messages error (`EMPTY_PARENT_CONTEXT`) is unreachable through normal agent flow - dead defensive code | D5 |
| P1-4 | `AgentAsTool` delegation has no explicit "worker done" completion gate - no separate child `EventReceiver` | D4 |
| P1-5 | PSF-3 resolved by hotfix 2026-07-02 - `orchest-provider` exposes `fakes::{FakeAsr, FakeTts}` behind `testing` feature. Demo doesn't exercise ASR/TTS | D11 |

### Pre-Seeded Findings Status

| PSF ID | Status | Notes |
|--------|--------|-------|
| PSF-1 | Post-1.0 (P1-1) | Path friction, not correctness. Recommend lib.rs re-export. |
| PSF-2 | Post-1.0 (P1-2) | Same as PSF-1. |
| PSF-3 | Resolved (P1-5) | Fixed by hotfix 2026-07-02. `orchest-provider::fakes` behind `testing` feature. Demo doesn't need ASR/TTS. |
