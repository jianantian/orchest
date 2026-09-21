# Research Pipeline seam gap analysis

## Executive summary

All deterministic contract, report, worker, supervisor/watcher, terminal-failure, and watcher-order checks passed. Seven supervised-delegation seam blockers and one release blocker remain open with owned GitHub issues; SB-6 is verified via AgentRun::start_with_watchers. The credential-gated live-provider run was not attempted because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent; v1.0 remains blocked by #258 until live verification passes.

## Readiness verdict

**Status:** `unverified`

**Reason:** The required live-provider run is not-run because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent. No waiver has been accepted; #258 blocks v1.0 until live verification passes.

**References:** `run-live-provider`

## Seam API checklist

| ID | API surface | Public path | Requirement | Status | Evidence | Findings |
| --- | --- | --- | --- | --- | --- | --- |
| API-1 | Watcher attachment and external steering | `orchest::run::RunHandle` | Attach watchers and exercise external steering through the supervisor handle. | `exercised` | `EV-public-api`, `EV-supervisor-source`, `EV-supervisor-watcher-test` | `SB-1`, `SB-2`, `SB-6` |
| API-2 | Supervisor event observation | `orchest::run::EventReceiver` | Observe terminal supervisor events without a fixed timeout. | `exercised` | `EV-failure-escalation-test`, `EV-public-api`, `EV-supervisor-watcher-test` | `P1-4` |
| API-3 | Worker restart policy | `orchest::run::SupervisionStrategy` | Configure restart and record whether a run-level failure restarts the delegated worker. | `exercised` | `EV-failure-escalation-test`, `EV-public-api`, `EV-supervisor-restart-source` | `SB-3` |
| API-4 | Watcher-originated steering | `orchest::run::WatcherAction` | Exercise Inject and Steer without claiming a child-worker target or cross-watcher action ordering. | `exercised` | `EV-public-api`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing` | `SB-2`, `SB-7` |
| API-5 | LLM watcher | `orchest::run::llm_watcher::LlmWatcher` | Use the public watcher builder and preserve nested-event formatting and missing-model seams. | `exercised` | `EV-llm-watcher-format-source`, `EV-public-api`, `EV-supervisor-source`, `EV-supervisor-watcher-test` | `P1-1`, `RB-1`, `SB-4`, `SB-8` |
| API-6 | Delegated context transfer | `orchest::tool::agent_as_tool::ContextMode` | Exercise Fresh and bounded Fork context modes. | `exercised` | `EV-public-api`, `EV-worker-test` | `P1-2`, `P1-3` |
| API-7 | Repeated failure termination | `orchest::hook::{Hook, HookAction, RepeatedFailureHookContext}` | Abort a repeated fatal tool failure through the public hook seam. | `exercised` | `EV-controlled-fault`, `EV-failure-escalation-test`, `EV-public-api`, `EV-worker-test` | `SB-3` |
| API-8 | Runtime event taxonomy | `orchest::events::RuntimeEvent` | Record forwarded nested events and watcher delivery boundaries. | `exercised` | `EV-agent-as-tool-forwarding-source`, `EV-failure-escalation-test`, `EV-primary-tool-context-source`, `EV-public-api`, `EV-secondary-delivery-source`, `EV-supervisor-watcher-test`, `EV-watcher-order-test` | `SB-4`, `SB-5`, `SB-6`, `SB-7`, `SB-8` |

## Findings summary

| ID | Title | Classification | Status | Issue |
| --- | --- | --- | --- | --- |
| P1-1 | LlmWatcher is not root re-exported | `post-1.0-backlog` | `deferred` | #256 |
| P1-2 | ContextMode is not root re-exported | `post-1.0-backlog` | `deferred` | #256 |
| P1-3 | Fork empty-parent context error is unreachable | `post-1.0-backlog` | `deferred` | #257 |
| P1-4 | Delegation has no explicit child completion receiver | `post-1.0-backlog` | `open` | #249 |
| P1-5 | Provider test fakes were previously inaccessible | `post-1.0-backlog` | `verified` | #196 |
| RB-1 | LlmWatcher builder panics without a model | `release-blocker` | `open` | #255 |
| SB-1 | Delegation does not expose a child RunHandle | `seam-blocker` | `open` | #249 |
| SB-2 | Steering targets the supervisor rather than delegated worker | `seam-blocker` | `open` | #249 |
| SB-3 | Restart does not cover run-level failure | `seam-blocker` | `open` | #251 |
| SB-4 | LlmWatcher does not format nested delegation events | `seam-blocker` | `open` | #250 |
| SB-5 | Secondary watcher subscribers can drop events | `seam-blocker` | `open` | #252 |
| SB-6 | No public start-with-watchers or pre-run pause seam | `seam-blocker` | `verified` | #253 |
| SB-7 | Watcher actions have no global registration-order guarantee | `seam-blocker` | `open` | #254 |
| SB-8 | Forwarded child events bypass attached watchers | `seam-blocker` | `open` | #250 |

### P1-1 — LlmWatcher is not root re-exported

**API surface:** `orchest::run::llm_watcher::LlmWatcher`

**Classification:** `post-1.0-backlog`

**Status:** `deferred`

**Description:** The watcher requires a deeper public import path than other related watcher APIs.

**Observed consequence:** The public surface is less discoverable but remains usable.

**Workaround:** Use the documented public module path.

**Evidence:** `EV-known-seams`, `EV-public-api`

**Action owner:** orchest-maintainers

**Action:** Add a convenience LlmWatcher re-export without removing the existing public path.

**Issue:** #256

**Verification status:** `not-applicable`

**Verification summary:** The reviewed release decision defers import-path polish to the post-1.0 backlog under #256.

**Verification commands:** —

**Verification evidence:** `EV-public-api`

### P1-2 — ContextMode is not root re-exported

**API surface:** `orchest::tool::agent_as_tool::ContextMode`

**Classification:** `post-1.0-backlog`

**Status:** `deferred`

**Description:** ContextMode requires a deeper public import path than adjacent APIs.

**Observed consequence:** The public surface is less discoverable but remains usable.

**Workaround:** Use the documented public module path.

**Evidence:** `EV-known-seams`, `EV-public-api`

**Action owner:** orchest-maintainers

**Action:** Add a convenience ContextMode re-export without removing the existing public path.

**Issue:** #256

**Verification status:** `not-applicable`

**Verification summary:** The reviewed release decision defers import-path polish to the post-1.0 backlog under #256.

**Verification commands:** —

**Verification evidence:** `EV-public-api`

### P1-3 — Fork empty-parent context error is unreachable

**API surface:** `orchest::tool::agent_as_tool::ContextMode::Fork`

**Classification:** `post-1.0-backlog`

**Status:** `deferred`

**Description:** Normal agent flow always supplies parent messages, leaving the empty-parent defensive error unreachable during delegation; public Tool::call_oneshot supplies the empty throwaway context needed to exercise it.

**Observed consequence:** Normal delegation cannot reach the branch, while a focused one-shot invocation can verify it without private context construction.

**Workaround:** Exercise the agent tool through public Tool::call_oneshot and retain separate normal Fresh and bounded Fork evidence.

**Evidence:** `EV-known-seams`, `EV-worker-test`

**Action owner:** orchest-maintainers

**Action:** Reconcile the empty-parent Fork error with the supported delegation contract.

**Issue:** #257

**Verification status:** `passed`

**Verification summary:** The focused test invokes the Fork agent tool through public Tool::call_oneshot, observes EMPTY_PARENT_CONTEXT without a model call, and separately passes Fresh and bounded Fork paths; #257 owns the deferred contract cleanup.

**Verification commands:** `cargo test -p research-pipeline-demo --test worker`

**Verification evidence:** `EV-worker-test`

### P1-4 — Delegation has no explicit child completion receiver

**API surface:** `orchest::run::EventReceiver`

**Classification:** `post-1.0-backlog`

**Status:** `open`

**Description:** Delegated worker completion is consumed by AgentAsTool and does not expose a separate child EventReceiver.

**Observed consequence:** Application-level completion can only be gated through the supervisor event receiver.

**Workaround:** Wait for the supervisor terminal event and observe nested completion through forwarded events.

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-failure-escalation-test`, `EV-known-seams`

**Action owner:** orchest-maintainers

**Action:** Expose delegated child completion through the public child control surface.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** The controlled failure loop receives forwarded child RunFailed and SubAgentFailed on the primary supervisor EventReceiver, then gates application completion on the supervisor RunCompleted terminal event without a fixed timeout.

**Verification commands:** `cargo test -p research-pipeline-demo --test failure_escalation`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-failure-escalation-test`

### P1-5 — Provider test fakes were previously inaccessible

**API surface:** `orchest-provider::fakes`

**Classification:** `post-1.0-backlog`

**Status:** `verified`

**Description:** The earlier fake-provider access gap was resolved by GitHub issue #196 and commit 8bb9a9b, which exposed FakeAsr and FakeTts behind the testing feature.

**Observed consequence:** Downstream deterministic tests and demos can consume provider fakes without maintaining local duplicates.

**Workaround:** No workaround remains necessary; use orchest-provider::fakes with the testing feature.

**Evidence:** `EV-known-seams`, `EV-provider-fakes-source`, `EV-provider-fakes-test`

**Action owner:** orchest-maintainers

**Action:** Resolved reusable FakeAsr and FakeTts access behind the testing feature.

**Issue:** #196

**Action revision:** `8bb9a9b`

**Verification status:** `passed`

**Verification summary:** All five focused FakeAsr/FakeTts tests passed on the current tree; the historical repair remains verified.

**Verification commands:** `cargo test -p orchest-provider --features testing fakes`

**Verification evidence:** `EV-provider-fakes-test`

### RB-1 — LlmWatcher builder panics without a model

**API surface:** `orchest::run::llm_watcher::LlmWatcherBuilder`

**Classification:** `release-blocker`

**Status:** `open`

**Description:** The current builder uses expect when model configuration is absent instead of returning a configuration error.

**Observed consequence:** A caller configuration mistake can panic library code before v1.0 freezes the API.

**Workaround:** Supply the model before calling the current infallible build method.

**Evidence:** `EV-known-seams`, `EV-llm-watcher-builder-source`

**Action owner:** orchest-maintainers

**Action:** Make LlmWatcherBuilder::build return a configuration error when model is absent.

**Issue:** #255

**Verification status:** `not-run`

**Verification summary:** Source inspection confirms the panic path; GitHub issue #255 owns the fallible API repair and verifier.

**Verification commands:** —

**Verification evidence:** `EV-llm-watcher-builder-source`

### SB-1 — Delegation does not expose a child RunHandle

**API surface:** `orchest::tool::agent_as_tool::AgentAsTool`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** AgentAsTool retains the delegated child handle internally, leaving application code with only the supervisor RunHandle.

**Observed consequence:** A caller cannot attach a watcher to or steer the delegated worker through a public handle.

**Workaround:** Attach to the supervisor and inspect forwarded nested events on the primary EventReceiver.

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-known-seams`, `EV-supervisor-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Expose a public delegated child run control and completion surface.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** The authentic public-surface attempt consumes and waits the only owned supervisor RunHandle while preserving the distinct child run id observed in forwarded evidence; no public child-handle constructor or lookup exists.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-supervisor-watcher-test`

### SB-2 — Steering targets the supervisor rather than delegated worker

**API surface:** `orchest::run::WatcherAction and orchest::run::RunHandle`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** Watcher and external Inject or Steer actions target the watched supervisor actor when delegation uses AgentAsTool.

**Observed consequence:** Application code cannot direct a message specifically to the delegated worker.

**Workaround:** Use supervisor-level steering and preserve the target mismatch as evidence.

**Evidence:** `EV-known-seams`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing`

**Action owner:** orchest-maintainers

**Action:** Route public delegated child inject and steer operations through the child control surface.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** The custom WatcherAction is triggered specifically by the supervisor-level research_worker ToolCallStarted event. WatcherAction::Inject, WatcherAction::Steer, RunHandle::inject_message, and RunHandle::steer all appeared in supervisor model histories and in none of the Fresh delegated-worker messages.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-supervisor-watcher-test`, `EV-watcher-action-routing`

### SB-3 — Restart does not cover run-level failure

**API surface:** `orchest::run::SupervisionStrategy`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** Restart reacts to actor crashes rather than RunFailed outcomes from tool errors, budgets, or max steps.

**Observed consequence:** A delegated worker can terminally fail without the configured restart policy recovering it.

**Workaround:** Use repeated-failure abort and supervisor escalation while recording restart absence.

**Evidence:** `EV-failure-escalation-test`, `EV-known-seams`, `EV-supervisor-restart-source`, `EV-worker-source`

**Action owner:** orchest-maintainers

**Action:** Apply bounded delegated-run restart policy to run-level failures and emit restart evidence.

**Issue:** #251

**Verification status:** `passed`

**Verification summary:** Restart with max_retries one was configured on the delegated worker. After a successful search_corpus step, indexed evidence showed the controlled RunFailed, SubAgentFailed, parent SUB_AGENT_RUN_FAILED, and supervisor RunCompleted escalation in order without panic; no RunRestarted event was captured, matching the ActorFailed-only source branch.

**Verification commands:** `cargo test -p research-pipeline-demo --test failure_escalation`

**Verification evidence:** `EV-failure-escalation-test`, `EV-supervisor-restart-source`

### SB-4 — LlmWatcher does not format nested delegation events

**API surface:** `orchest::run::llm_watcher::LlmWatcher`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** LlmWatcher formatting has no dedicated handler for SubAgentEvent, child lifecycle, or child run events.

**Observed consequence:** If a nested event reaches this formatter it falls back to debug-style text; the current attached-watcher flow does not receive nested events at all because of SB-8.

**Workaround:** Use the public watcher unchanged for supervisor events and render primary-receiver nested events separately for human traces.

**Evidence:** `EV-known-seams`, `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Add structured LlmWatcher formatting for delegated child lifecycle and runtime events.

**Issue:** #250

**Verification status:** `not-run`

**Verification summary:** Source inspection confirms the latent formatting gap; no nested-format runtime claim is made because SB-8 bypasses attached LlmWatcher delivery.

**Verification commands:** —

**Verification evidence:** `EV-llm-watcher-format-source`

### SB-5 — Secondary watcher subscribers can drop events

**API surface:** `orchest::events::RuntimeEvent`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** Secondary watcher subscriptions use non-blocking delivery and can lose events under backpressure.

**Observed consequence:** A watcher has no recovery path for dropped events when its channel is full.

**Workaround:** Use sufficient capacity for deterministic no-drop tests and record the boundary.

**Evidence:** `EV-known-seams`, `EV-secondary-delivery-source`, `EV-watcher-order-test`

**Action owner:** orchest-maintainers

**Action:** Provide an observable watcher event-loss recovery or replay contract.

**Issue:** #252

**Verification status:** `passed`

**Verification summary:** With capacity 1024, both terminal-complete watchers recorded the same indexed sequence and the primary receiver observed no EventsDropped. This is a no-drop scenario only; source inspection preserves the possible try_send loss under backpressure.

**Verification commands:** `cargo test -p research-pipeline-demo --test watcher_order`

**Verification evidence:** `EV-secondary-delivery-source`, `EV-watcher-order-test`

### SB-6 — No public start-with-watchers or pre-run pause seam

**API surface:** `orchest::run::AgentRun::start_with_watchers`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** AgentRun::start schedules execution before application code can attach watchers, and no public pre-run pause or watcher constructor seam exists.

**Observed consequence:** Application code cannot guarantee observation beginning with the first runtime event or first model call.

**Workaround:** Use AgentRun::start_with_watchers for first-event guarantees; post-start attach_watcher remains best-effort.

**Evidence:** `EV-known-seams`, `EV-supervisor-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Added AgentRun::start_with_watchers which pre-wires watchers before RunStarted.

**Issue:** #253

**Verification status:** `passed`

**Verification summary:** Live helper uses AgentRun::start_with_watchers so declared watchers observe from RunStarted; post-start attach_watcher remains best-effort. Deterministic suite still passes.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-supervisor-source`, `EV-supervisor-watcher-test`

### SB-7 — Watcher actions have no global registration-order guarantee

**API surface:** `orchest::run::WatcherAction`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** Each watcher runs independently, so actions returned by multiple watchers do not have a global registration-order execution guarantee.

**Observed consequence:** Concurrent Inject, Steer, or Abort actions cannot be attributed to watcher registration order.

**Workaround:** Test per-watcher FIFO separately and do not claim cross-watcher action ordering.

**Evidence:** `EV-known-seams`, `EV-watcher-order-test`, `EV-watcher-task-topology`

**Action owner:** orchest-maintainers

**Action:** Define and implement deterministic arbitration for actions from multiple watchers.

**Issue:** #254

**Verification status:** `passed`

**Verification summary:** The delivery-only CountingWatchers preserve FIFO and receive equivalent indexed sequences, but return no actions. Source shows one independent Tokio task per watcher and applies each action after its own on_event future, so registration order is not a global action-order contract.

**Verification commands:** `cargo test -p research-pipeline-demo --test watcher_order`

**Verification evidence:** `EV-watcher-order-test`, `EV-watcher-task-topology`

### SB-8 — Forwarded child events bypass attached watchers

**API surface:** `orchest::run::EventReceiver and orchest::run::RunHandle::attach_watcher`

**Classification:** `seam-blocker`

**Status:** `open`

**Description:** ToolContext.event_tx is the primary subscriber sender, and AgentAsTool forwards child events directly through it rather than through attached watcher subscription channels.

**Observed consequence:** The primary supervisor EventReceiver can observe SubAgentEvent values, but a supervisor-attached watcher cannot monitor delegated-worker events.

**Workaround:** Consume primary-receiver nested events separately and treat attached watchers as supervisor-event observers.

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Deliver forwarded delegated child events to attached watcher subscriptions.

**Issue:** #250

**Verification status:** `passed`

**Verification summary:** The primary EventReceiver observed forwarded child completion; after both the custom watcher and LlmWatcher wrapper completed processing the terminal supervisor event, neither completed event vector contained a SubAgentEvent. LlmWatcher prompt evidence is limited to prompts recorded before its model call and is not used as on_event completion evidence.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test`

## Verification evidence

| Finding | Status | Commands | Evidence | Summary |
| --- | --- | --- | --- | --- |
| P1-1 | `not-applicable` | — | `EV-public-api` | The reviewed release decision defers import-path polish to the post-1.0 backlog under #256. |
| P1-2 | `not-applicable` | — | `EV-public-api` | The reviewed release decision defers import-path polish to the post-1.0 backlog under #256. |
| P1-3 | `passed` | `cargo test -p research-pipeline-demo --test worker` | `EV-worker-test` | The focused test invokes the Fork agent tool through public Tool::call_oneshot, observes EMPTY_PARENT_CONTEXT without a model call, and separately passes Fresh and bounded Fork paths; #257 owns the deferred contract cleanup. |
| P1-4 | `passed` | `cargo test -p research-pipeline-demo --test failure_escalation` | `EV-agent-as-tool-forwarding-source`, `EV-failure-escalation-test` | The controlled failure loop receives forwarded child RunFailed and SubAgentFailed on the primary supervisor EventReceiver, then gates application completion on the supervisor RunCompleted terminal event without a fixed timeout. |
| P1-5 | `passed` | `cargo test -p orchest-provider --features testing fakes` | `EV-provider-fakes-test` | All five focused FakeAsr/FakeTts tests passed on the current tree; the historical repair remains verified. |
| RB-1 | `not-run` | — | `EV-llm-watcher-builder-source` | Source inspection confirms the panic path; GitHub issue #255 owns the fallible API repair and verifier. |
| SB-1 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-supervisor-watcher-test` | The authentic public-surface attempt consumes and waits the only owned supervisor RunHandle while preserving the distinct child run id observed in forwarded evidence; no public child-handle constructor or lookup exists. |
| SB-2 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-supervisor-watcher-test`, `EV-watcher-action-routing` | The custom WatcherAction is triggered specifically by the supervisor-level research_worker ToolCallStarted event. WatcherAction::Inject, WatcherAction::Steer, RunHandle::inject_message, and RunHandle::steer all appeared in supervisor model histories and in none of the Fresh delegated-worker messages. |
| SB-3 | `passed` | `cargo test -p research-pipeline-demo --test failure_escalation` | `EV-failure-escalation-test`, `EV-supervisor-restart-source` | Restart with max_retries one was configured on the delegated worker. After a successful search_corpus step, indexed evidence showed the controlled RunFailed, SubAgentFailed, parent SUB_AGENT_RUN_FAILED, and supervisor RunCompleted escalation in order without panic; no RunRestarted event was captured, matching the ActorFailed-only source branch. |
| SB-4 | `not-run` | — | `EV-llm-watcher-format-source` | Source inspection confirms the latent formatting gap; no nested-format runtime claim is made because SB-8 bypasses attached LlmWatcher delivery. |
| SB-5 | `passed` | `cargo test -p research-pipeline-demo --test watcher_order` | `EV-secondary-delivery-source`, `EV-watcher-order-test` | With capacity 1024, both terminal-complete watchers recorded the same indexed sequence and the primary receiver observed no EventsDropped. This is a no-drop scenario only; source inspection preserves the possible try_send loss under backpressure. |
| SB-6 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-supervisor-source`, `EV-supervisor-watcher-test` | Live helper uses AgentRun::start_with_watchers so declared watchers observe from RunStarted; post-start attach_watcher remains best-effort. Deterministic suite still passes. |
| SB-7 | `passed` | `cargo test -p research-pipeline-demo --test watcher_order` | `EV-watcher-order-test`, `EV-watcher-task-topology` | The delivery-only CountingWatchers preserve FIFO and receive equivalent indexed sequences, but return no actions. Source shows one independent Tokio task per watcher and applies each action after its own on_event future, so registration order is not a global action-order contract. |
| SB-8 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test` | The primary EventReceiver observed forwarded child completion; after both the custom watcher and LlmWatcher wrapper completed processing the terminal supervisor event, neither completed event vector contained a SubAgentEvent. LlmWatcher prompt evidence is limited to prompts recorded before its model call and is not used as on_event completion evidence. |

## Run evidence

Revision `git:self` denotes the commit containing the canonical findings file and is reserved for post-commit verification evidence.

| ID | Kind | Status | Required | Command | Date | Revision | Provider | Model | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| run-deterministic-runtime | `test` | `passed` | yes | `cargo test -p research-pipeline-demo` | 2026-07-31 | `git:self` | — | — | `EV-demo-package-test` |
| run-failure-escalation-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test failure_escalation` | 2026-07-31 | `git:self` | — | — | `EV-failure-escalation-test` |
| run-fixture-contract | `fixture` | `passed` | yes | `cargo test -p research-pipeline-demo --test findings_contract` | 2026-07-31 | `git:self` | — | — | `EV-fixture-contract-test` |
| run-live-provider | `live-provider` | `not-run` | yes | — | — | — | configured-by-env | configured-by-env | — |
| run-provider-fakes-verification | `test` | `passed` | yes | `cargo test -p orchest-provider --features testing fakes` | 2026-07-31 | `git:self` | — | — | `EV-provider-fakes-test` |
| run-report-smoke | `smoke` | `passed` | yes | `cargo run -p research-pipeline-demo --bin seam-report -- check --findings examples/demo/research-pipeline/findings.json --report docs/iteration/v0_11/seam-gap-analysis.md` | 2026-07-31 | `git:self` | — | — | `EV-report-smoke` |
| run-supervisor-watcher-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 2026-07-31 | `git:self` | — | — | `EV-supervisor-watcher-test` |
| run-watcher-order-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test watcher_order` | 2026-07-31 | `git:self` | — | — | `EV-watcher-order-test` |
| run-worker-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test worker` | 2026-07-31 | `git:self` | — | — | `EV-worker-test` |

#### run-deterministic-runtime

The complete provider-independent demo package passed 69 tests across evidence contract, isolated report path resolution and read-only checking, worker, supervisor/watcher, terminal failure, and watcher ordering; the distinct credential-gated live-provider smoke remained ignored.

#### run-failure-escalation-deterministic

The worker completed search_corpus before triggering the controlled Fatal/Unsafe fault. Indexed primary-receiver boundaries then proved nested fault failure, nested RunFailed, SubAgentFailed, parent SUB_AGENT_RUN_FAILED, and supervisor escalation in strict order. Restart was configured but no RunRestarted event was captured.

#### run-fixture-contract

All 42 contract tests passed, including canonical final-run provenance, closed enums, graph references, canonical paths, calendar dates, strict revisions and symbols, lifecycle/readiness rules, systematic privacy, and action ownership.

#### run-live-provider

The ignored tests/smoke.rs live-provider target was not executed because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent. No waiver is granted; GitHub issue #258 owns the v1.0 live-verification gate.

#### run-provider-fakes-verification

Five FakeAsr/FakeTts tests passed on the containing commit, confirming the historical provider-fake access gap remains resolved.

#### run-report-smoke

On the containing commit, the seam-report CLI rendered in memory and matched the generated Markdown byte-for-byte without writing the report.

#### run-supervisor-watcher-deterministic

Eight deterministic tests passed: a probe activates both watcher subscriptions before second-step delegation, terminal-complete custom and LLM watcher vectors prove primary-receiver-only nested forwarding, each public steering path reaches only the supervisor history, Fresh and bounded Fork delegate, and the live-shaped helper uses start_with_watchers for first-event attachment.

#### run-watcher-order-deterministic

Two supervisor watchers with capacity 1024 each observed the expected stable milestone subsequence and the same complete indexed sequence with no EventsDropped observed. The test returns no actions and makes no action-order claim.

#### run-worker-deterministic

Eight deterministic worker tests passed: corpus tools, fatal unsafe fault shape, threshold-one abort termination, Fresh and bounded Fork context, empty-parent error, and event rendering.

### Live-provider boundary

- **run-live-provider** — `not-run`; provider `configured-by-env`; model `configured-by-env`; The ignored tests/smoke.rs live-provider target was not executed because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent. No waiver is granted; GitHub issue #258 owns the v1.0 live-verification gate.

## Evidence catalogue

| ID | Kind | Locator | Run | Command | Result | Summary |
| --- | --- | --- | --- | --- | --- | --- |
| EV-agent-as-tool-forwarding-source | `source` | `crates/orchest/src/tool/agent_as_tool.rs` · `AgentAsTool::run_child_attempt` | — | — | — | AgentAsTool forwards child RuntimeEvent values as SubAgentEvent by sending directly through ToolContext.event_tx and retains the child RunHandle internally until wait completes. |
| EV-controlled-fault | `source` | `examples/demo/research-pipeline/src/fault.rs` · `FaultTriggerTool::execute and ControlledFaultAbortHook::on_repeated_failure` | — | — | — | The controlled fault returns Fatal with Unsafe retry semantics, and the matching repeated-failure hook aborts the run. |
| EV-demo-package-test | `test` | `examples/demo/research-pipeline/Cargo.toml` | run-deterministic-runtime | `cargo test -p research-pipeline-demo` | 69 passed; 0 failed; 1 credential-gated live smoke ignored | The complete provider-independent Research Pipeline package passed without executing the ignored credential-gated provider smoke or writing the tracked canonical report. |
| EV-evidence-contract | `documentation` | `docs/archive/iteration/v0_11/finding-evidence-contract-design.md` · `Stable Seam Finding Evidence Design` | — | — | — | The evidence contract defines lifecycle, reference, and readiness requirements. |
| EV-failure-escalation-test | `test` | `examples/demo/research-pipeline/tests/failure_escalation.rs` · `controlled_worker_failure_reaches_supervisor_escalation_without_restart_or_panic` | run-failure-escalation-deterministic | `cargo test -p research-pipeline-demo --test failure_escalation` | 1 passed; 0 failed; ordered search-to-escalation chain; no RunRestarted captured | The focused target gates completion on the public supervisor EventReceiver terminal event, requires completed search_corpus before fault_trigger, and extracts strict indices for nested Fatal/Unsafe failure, threshold-one abort RunFailed, SubAgentFailed, parent SUB_AGENT_RUN_FAILED, and supervisor escalation. It separately proves configured restart absence and no hook panic. |
| EV-fixture-contract-test | `runtime-output` | `examples/demo/research-pipeline/tests/findings_contract.rs` · `canonical_final_executed_rows_use_the_containing_commit_revision` | run-fixture-contract | `cargo test -p research-pipeline-demo --test findings_contract` | 42 passed; 0 failed | The strict canonical contract fixture target passed, including the containing-commit provenance guard for final executed rows. |
| EV-known-seams | `documentation` | `docs/archive/iteration/v0_11/design-decisions.md` · `Pre-Identified Seam Gap Findings Summary` | — | — | — | Locked design decisions list the pre-identified seams and their stable ids. |
| EV-llm-watcher-builder-source | `source` | `crates/orchest/src/run/llm_watcher.rs` · `LlmWatcherBuilder::build` | — | — | — | LlmWatcherBuilder::build uses expect when no model was configured, causing a public library panic instead of a configuration error. |
| EV-llm-watcher-format-source | `source` | `crates/orchest/src/run/llm_watcher.rs` · `format_event` | — | — | — | LlmWatcher::format_event has no dedicated nested delegation arms; in this public flow SB-8 prevents attached LlmWatcher delivery, so this remains source evidence rather than an executed nested-format claim. |
| EV-primary-tool-context-source | `source` | `crates/orchest/src/run/actor.rs` · `run_tool_and_handoff_phase` | — | — | — | The run actor constructs ToolContext.event_tx from the primary subscriber sender rather than broadcasting tool-originated events to every attached subscription. |
| EV-provider-fakes-source | `source` | `crates/orchest-provider/src/fakes.rs` · `FakeAsr and FakeTts` | — | — | — | The orchest-provider wall exposes deterministic FakeAsr and FakeTts implementations behind the testing feature. |
| EV-provider-fakes-test | `test` | `crates/orchest-provider/src/fakes.rs` · `tests` | run-provider-fakes-verification | `cargo test -p orchest-provider --features testing fakes` | 5 passed; 0 failed; 7 filtered out | The focused provider-fakes test filter passed all five FakeAsr/FakeTts tests. |
| EV-public-api | `documentation` | `docs/archive/iteration/v0_11/implementation-plan.md` · `Public paths` | — | — | — | The v0.11 implementation overview fixes the public imports that the demo may use. |
| EV-report-smoke | `smoke-run` | `examples/demo/research-pipeline/src/bin/seam-report.rs` · `Command::Check` | run-report-smoke | `cargo run -p research-pipeline-demo --bin seam-report -- check --findings examples/demo/research-pipeline/findings.json --report docs/iteration/v0_11/seam-gap-analysis.md` | report current; byte-for-byte match | The seam-report staleness check passed against the deterministic Markdown projection. |
| EV-secondary-delivery-source | `source` | `crates/orchest/src/run/actor.rs` · `emit` | — | — | — | The run actor blocks for the primary subscriber but uses try_send for secondary watcher subscribers, recording EventsDropped on observed channel-full loss. |
| EV-supervisor-restart-source | `source` | `crates/orchest/src/run/supervisor.rs` · `SupervisorActor::handle_supervisor_evt` | — | — | — | The runtime emits RunRestarted only from SupervisorActor's ActorFailed branch; clean WorkerActor termination after RunFailed follows ActorTerminated instead. |
| EV-supervisor-source | `source` | `examples/demo/research-pipeline/src/supervisor.rs` · `build_supervisor and start_with_live_watchers` | — | — | — | The demo builds the worker through Worker::as_tool, registers it with an Orchest supervisor, instructs the fault path to call search_corpus before fault_trigger and then return an escalation summary without retrying, exposes only the supervisor RunHandle, and starts with AgentRun::start_with_watchers so declared watchers observe from RunStarted. |
| EV-supervisor-watcher-test | `test` | `examples/demo/research-pipeline/tests/supervisor_watcher.rs` · `activated_watchers_prove_nested_routing_and_applied_supervisor_actions` | run-supervisor-watcher-deterministic | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 8 passed; 0 failed; both watcher terminal-complete vectors prove SB-8 without a correctness timeout | The focused target gates the first supervisor call, releases a harmless probe so queued subscriptions activate, and releases second-step delegation only after both watcher wrappers complete its ModelCallStarted event. It proves each public steering path changes only supervisor history, observes forwarded child completion on the primary EventReceiver, and reaches terminal-complete custom and LLM watcher vectors containing no nested event. |
| EV-watcher-action-routing | `source` | `crates/orchest/src/run/supervisor.rs` · `reattach_watcher` | — | — | — | Watcher Inject and Steer actions are cast to the currently watched supervisor worker actor. |
| EV-watcher-order-test | `test` | `examples/demo/research-pipeline/tests/watcher_order.rs` · `two_watchers_preserve_fifo_and_match_the_complete_no_drop_sequence` | run-watcher-order-deterministic | `cargo test -p research-pipeline-demo --test watcher_order` | 1 passed; 0 failed; sequences equal; no action-order inference | The focused target attaches two CountingWatchers to one supervisor RunHandle, proves the expected stable FIFO milestones in each stream, and compares their complete indexed sequences after terminal processing with capacity 1024 and no observed EventsDropped. |
| EV-watcher-source | `source` | `examples/demo/research-pipeline/src/watcher.rs` · `stable_event_key and CountingWatcher::on_event and RecordingActionWatcher::on_event and RecordingLlmWatcher::on_event` | — | — | — | The custom watcher returns one public WatcherAction only for the supervisor-level research_worker ToolCallStarted event. The action and LLM wrappers record completion through terminal supervisor events; CountingWatcher maps every accepted event to a stable key and always returns Continue for delivery-only evidence. |
| EV-watcher-task-topology | `source` | `crates/orchest/src/run/supervisor.rs` · `reattach_watcher` | — | — | — | Each registered watcher is processed in its own Tokio task, and its action is applied only after that independent on_event future completes; registration order is not serialized into action application order. |
| EV-worker-events | `source` | `examples/demo/research-pipeline/src/events.rs` · `render_event` | — | — | — | The event renderer labels model turns, tool calls, tool results, nested worker events, and terminal status from RuntimeEvent values. |
| EV-worker-source | `source` | `examples/demo/research-pipeline/src/worker.rs` · `Worker::from_paths` | — | — | — | Worker::from_paths registers search_corpus, read_file, write_draft, and fault_trigger, configures repeated_failure_threshold(1), and sets public SupervisionStrategy::Restart with one retry. |
| EV-worker-test | `test` | `examples/demo/research-pipeline/tests/worker.rs` · `worker_threshold_and_abort_hook_produce_terminal_run_failed` | run-worker-deterministic | `cargo test -p research-pipeline-demo --test worker` | 8 passed; 0 failed; P1-3 exercised through public Tool::call_oneshot | The focused worker target separately proves the tool error, threshold-plus-hook terminal failure, Fresh and bounded Fork histories, the empty-parent error through public Tool::call_oneshot without a model call, deterministic tools, and event rendering. |

## v1.0 and Multivac M2 implications

### v1.0

Readiness remains `unverified`: The required live-provider run is not-run because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent. No waiver has been accepted; #258 blocks v1.0 until live verification passes.

Required live-provider gates: `run-live-provider`.

Unresolved release blockers:

- RB-1 — LlmWatcher builder panics without a model (`open`, #255)

### Multivac M2

Unresolved supervised-delegation seam blockers:

- SB-1 — Delegation does not expose a child RunHandle (`open`, #249)
- SB-2 — Steering targets the supervisor rather than delegated worker (`open`, #249)
- SB-3 — Restart does not cover run-level failure (`open`, #251)
- SB-4 — LlmWatcher does not format nested delegation events (`open`, #250)
- SB-5 — Secondary watcher subscribers can drop events (`open`, #252)
- SB-7 — Watcher actions have no global registration-order guarantee (`open`, #254)
- SB-8 — Forwarded child events bypass attached watchers (`open`, #250)

### Post-1.0 backlog

- P1-1 — LlmWatcher is not root re-exported (`deferred`, #256)
- P1-2 — ContextMode is not root re-exported (`deferred`, #256)
- P1-3 — Fork empty-parent context error is unreachable (`deferred`, #257)
- P1-4 — Delegation has no explicit child completion receiver (`open`, #249)
- P1-5 — Provider test fakes were previously inaccessible (`verified`, #196)
