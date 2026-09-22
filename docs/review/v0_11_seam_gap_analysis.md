# Research Pipeline seam gap analysis

## Executive summary

All deterministic contract, report, worker, supervisor/watcher, terminal-failure, and watcher-order checks passed. 2 supervised-delegation seam blockers and one release blocker remain open with owned GitHub issues; SB-1/SB-2/P1-4 are verified via ChildRunHandle / ChildRunRegistry; SB-4 and SB-8 are verified via ToolContext::emit_event fan-out and structured LlmWatcher nested formatting; SB-5 is verified via EventSink coalesced loss recovery; SB-6 is verified via AgentRun::start_with_watchers; SB-7 is verified via ActionArbitrator. The credential-gated live-provider run was not attempted because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent; v1.0 remains blocked by #258 until live verification passes.

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
| P1-4 | Delegation has no explicit child completion receiver | `post-1.0-backlog` | `verified` | #249 |
| P1-5 | Provider test fakes were previously inaccessible | `post-1.0-backlog` | `verified` | #196 |
| RB-1 | LlmWatcher builder panics without a model | `release-blocker` | `open` | #255 |
| SB-1 | Delegation does not expose a child RunHandle | `seam-blocker` | `verified` | #249 |
| SB-2 | Steering targets the supervisor rather than delegated worker | `seam-blocker` | `verified` | #249 |
| SB-3 | Restart does not cover run-level failure | `seam-blocker` | `open` | #251 |
| SB-4 | LlmWatcher does not format nested delegation events | `seam-blocker` | `verified` | #250 |
| SB-5 | Secondary watcher subscribers can drop events | `seam-blocker` | `verified` | #252 |
| SB-6 | No public start-with-watchers or pre-run pause seam | `seam-blocker` | `verified` | #253 |
| SB-7 | Watcher actions have no global registration-order guarantee | `seam-blocker` | `verified` | #254 |
| SB-8 | Forwarded child events bypass attached watchers | `seam-blocker` | `verified` | #250 |

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

**Status:** `verified`

**Description:** Delegated worker completion is consumed by AgentAsTool and does not expose a separate child EventReceiver.

**Observed consequence:** Pre-fix: application-level completion could only be gated through the supervisor event receiver.

**Workaround:** Await ChildRunHandle::wait_completion (or await_delegated_worker_completion) without draining the supervisor EventReceiver.

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-failure-escalation-test`, `EV-known-seams`

**Action owner:** orchest-maintainers

**Action:** Exposed ChildRunHandle::wait_completion backed by a watch channel published from AgentAsTool on RunCompleted/RunFailed.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** Child completion/failure is awaitable via ChildRunHandle::wait_completion without consuming the supervisor EventReceiver; failure_escalation still gates supervisor RunCompleted on the primary channel.

**Verification commands:** `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test failure_escalation`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-failure-escalation-test`

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

**Status:** `verified`

**Description:** AgentAsTool retains the delegated child handle internally, leaving application code with only the supervisor RunHandle.

**Observed consequence:** Pre-fix: callers could not attach a watcher to or steer the delegated worker through a public handle.

**Workaround:** Use RunHandle::child after SubAgentStarted for inject/steer/completion; supervisor-level steering remains available on the supervisor handle.

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-known-seams`, `EV-supervisor-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Exposed ChildRunHandle via ChildRunRegistry on the supervisor RunHandle; AgentAsTool registers each delegated child for inject/steer/abort/subscribe/attach_watcher and wait_completion.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** RunHandle::child resolves a public ChildRunHandle after SubAgentStarted; supervisor_watcher and orchest child_control tests prove lookup, child-target inject/steer, and independent wait_completion.

**Verification commands:** `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test`

### SB-2 — Steering targets the supervisor rather than delegated worker

**API surface:** `orchest::run::WatcherAction and orchest::run::RunHandle`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Watcher and external Inject or Steer actions target the watched supervisor actor when delegation uses AgentAsTool.

**Observed consequence:** Pre-fix: application code could not direct a message specifically to the delegated worker; supervisor-level Inject/Steer still target the supervisor.

**Workaround:** Use ChildRunHandle::inject_message / steer for worker-targeted control; keep RunHandle / WatcherAction paths for supervisor-targeted steering.

**Evidence:** `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-known-seams`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing`

**Action owner:** orchest-maintainers

**Action:** Routed public delegated child inject and steer through ChildRunHandle; supervisor-level WatcherAction and RunHandle steering remain supervisor-targeted.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** child_control_surface_targets_worker_and_awaits_completion proves child inject/steer land only in the child conversation; existing supervisor_watcher steering path tests still prove supervisor-only targeting for WatcherAction and RunHandle paths.

**Verification commands:** `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing`

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

**Status:** `verified`

**Description:** LlmWatcher formatting has no dedicated handler for SubAgentEvent, child lifecycle, or child run events.

**Observed consequence:** Pre-fix: nested events fell back to debug-style text and attached watchers missed them because of SB-8.

**Workaround:** Use the public watcher unchanged for supervisor events and render primary-receiver nested events separately for human traces.

**Evidence:** `EV-known-seams`, `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Added structured LlmWatcher formatting for delegated child lifecycle and runtime events.

**Issue:** #250

**Verification status:** `passed`

**Verification summary:** LlmWatcher::format_event emits structured text for SubAgentStarted/Completed/Failed, SubAgentEvent, and ChildRunEvent. Attached LlmWatcher receives and formats nested events in supervisor_watcher after SB-8 repair (unit coverage lives in orchest::run::llm_watcher).

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test`

### SB-5 — Secondary watcher subscribers can drop events

**API surface:** `orchest::events::EventSink / orchest::events::RuntimeEvent::EventsDropped`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Secondary watcher subscriptions use non-blocking delivery and can lose events under backpressure.

**Observed consequence:** A watcher has no recovery path for dropped events when its channel is full.

**Workaround:** Prefer capacity that avoids saturation; when EventsDropped arrives on a secondary, treat the gap as bounded (payloads not replayed) and continue or resubscribe for future events.

**Evidence:** `EV-event-loss-recovery-test`, `EV-known-seams`, `EV-secondary-delivery-source`, `EV-watcher-order-test`

**Action owner:** orchest-maintainers

**Action:** Secondary loss coalesces into O(1) pending state and surfaces as EventsDropped { subscriber_id, count, from_seq, to_seq } on the affected secondary once capacity frees (mirror to primary best-effort). Lost payloads are not replayed.

**Issue:** #252

**Verification status:** `passed`

**Verification summary:** Saturation unit tests prove coalesced secondary EventsDropped with sequence metadata, flush-before-resume, and O(1) pending state; watcher_order remains the no-drop FIFO equivalence check.

**Verification commands:** `cargo test -p orchest --lib events::`<br>`cargo test -p research-pipeline-demo --test watcher_order`

**Verification evidence:** `EV-event-loss-recovery-test`, `EV-secondary-delivery-source`, `EV-watcher-order-test`

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

**Status:** `verified`

**Description:** Each watcher runs independently, so actions returned by multiple watchers do not have a global registration-order execution guarantee.

**Observed consequence:** Concurrent Inject, Steer, or Abort actions cannot be attributed to watcher registration order.

**Workaround:** N/A — arbitration contract is public and enforced.

**Evidence:** `EV-known-seams`, `EV-watcher-action-arbitration-test`, `EV-watcher-order-test`, `EV-watcher-task-topology`

**Action owner:** orchest-maintainers

**Action:** Implemented deterministic multi-watcher action arbitration via ActionArbitrator / arbitrate_watcher_actions.

**Issue:** #254

**Verification status:** `passed`

**Verification summary:** Fan-out waves gate actions until the delivery cohort completes; Abort wins; otherwise Inject/Steer apply in registration order. Adversarial gated arbitrator tests are schedule-independent; watcher_order remains the delivery-FIFO check.

**Verification commands:** `cargo test -p orchest --lib watcher_arbitration`<br>`cargo test -p research-pipeline-demo --test watcher_order`

**Verification evidence:** `EV-watcher-action-arbitration-test`, `EV-watcher-order-test`, `EV-watcher-task-topology`

### SB-8 — Forwarded child events bypass attached watchers

**API surface:** `orchest::run::EventReceiver and orchest::run::RunHandle::attach_watcher`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** ToolContext.event_tx is the primary subscriber sender, and AgentAsTool forwards child events directly through it rather than through attached watcher subscription channels.

**Observed consequence:** Pre-fix: primary EventReceiver observed SubAgentEvent values but supervisor-attached watchers could not.

**Workaround:** Consume primary-receiver nested events separately and treat attached watchers as supervisor-event observers.

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Delivered forwarded delegated child events to attached watcher subscriptions via ToolContext::emit_event.

**Issue:** #250

**Verification status:** `passed`

**Verification summary:** Attached watchers receive forwarded SubAgentEvent values via ToolContext::emit_event; primary remains single-delivery; activation-before-delegation holds. Deterministic supervisor_watcher proves nested delivery on custom and LlmWatcher vectors.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test`

## Verification evidence

| Finding | Status | Commands | Evidence | Summary |
| --- | --- | --- | --- | --- |
| P1-1 | `not-applicable` | — | `EV-public-api` | The reviewed release decision defers import-path polish to the post-1.0 backlog under #256. |
| P1-2 | `not-applicable` | — | `EV-public-api` | The reviewed release decision defers import-path polish to the post-1.0 backlog under #256. |
| P1-3 | `passed` | `cargo test -p research-pipeline-demo --test worker` | `EV-worker-test` | The focused test invokes the Fork agent tool through public Tool::call_oneshot, observes EMPTY_PARENT_CONTEXT without a model call, and separately passes Fresh and bounded Fork paths; #257 owns the deferred contract cleanup. |
| P1-4 | `passed` | `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test failure_escalation`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-failure-escalation-test` | Child completion/failure is awaitable via ChildRunHandle::wait_completion without consuming the supervisor EventReceiver; failure_escalation still gates supervisor RunCompleted on the primary channel. |
| P1-5 | `passed` | `cargo test -p orchest-provider --features testing fakes` | `EV-provider-fakes-test` | All five focused FakeAsr/FakeTts tests passed on the current tree; the historical repair remains verified. |
| RB-1 | `not-run` | — | `EV-llm-watcher-builder-source` | Source inspection confirms the panic path; GitHub issue #255 owns the fallible API repair and verifier. |
| SB-1 | `passed` | `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test` | RunHandle::child resolves a public ChildRunHandle after SubAgentStarted; supervisor_watcher and orchest child_control tests prove lookup, child-target inject/steer, and independent wait_completion. |
| SB-2 | `passed` | `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing` | child_control_surface_targets_worker_and_awaits_completion proves child inject/steer land only in the child conversation; existing supervisor_watcher steering path tests still prove supervisor-only targeting for WatcherAction and RunHandle paths. |
| SB-3 | `passed` | `cargo test -p research-pipeline-demo --test failure_escalation` | `EV-failure-escalation-test`, `EV-supervisor-restart-source` | Restart with max_retries one was configured on the delegated worker. After a successful search_corpus step, indexed evidence showed the controlled RunFailed, SubAgentFailed, parent SUB_AGENT_RUN_FAILED, and supervisor RunCompleted escalation in order without panic; no RunRestarted event was captured, matching the ActorFailed-only source branch. |
| SB-4 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test` | LlmWatcher::format_event emits structured text for SubAgentStarted/Completed/Failed, SubAgentEvent, and ChildRunEvent. Attached LlmWatcher receives and formats nested events in supervisor_watcher after SB-8 repair (unit coverage lives in orchest::run::llm_watcher). |
| SB-5 | `passed` | `cargo test -p orchest --lib events::`<br>`cargo test -p research-pipeline-demo --test watcher_order` | `EV-event-loss-recovery-test`, `EV-secondary-delivery-source`, `EV-watcher-order-test` | Saturation unit tests prove coalesced secondary EventsDropped with sequence metadata, flush-before-resume, and O(1) pending state; watcher_order remains the no-drop FIFO equivalence check. |
| SB-6 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-supervisor-source`, `EV-supervisor-watcher-test` | Live helper uses AgentRun::start_with_watchers so declared watchers observe from RunStarted; post-start attach_watcher remains best-effort. Deterministic suite still passes. |
| SB-7 | `passed` | `cargo test -p orchest --lib watcher_arbitration`<br>`cargo test -p research-pipeline-demo --test watcher_order` | `EV-watcher-action-arbitration-test`, `EV-watcher-order-test`, `EV-watcher-task-topology` | Fan-out waves gate actions until the delivery cohort completes; Abort wins; otherwise Inject/Steer apply in registration order. Adversarial gated arbitrator tests are schedule-independent; watcher_order remains the delivery-FIFO check. |
| SB-8 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test` | Attached watchers receive forwarded SubAgentEvent values via ToolContext::emit_event; primary remains single-delivery; activation-before-delegation holds. Deterministic supervisor_watcher proves nested delivery on custom and LlmWatcher vectors. |

## Run evidence

Revision `git:self` denotes the commit containing the canonical findings file and is reserved for post-commit verification evidence.

| ID | Kind | Status | Required | Command | Date | Revision | Provider | Model | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| run-child-control-unit | `test` | `passed` | yes | `cargo test -p orchest --lib child_control` | 2026-09-22 | `git:self` | — | — | `EV-child-control-unit-test` |
| run-deterministic-runtime | `test` | `passed` | yes | `cargo test -p research-pipeline-demo` | 2026-07-31 | `git:self` | — | — | `EV-demo-package-test` |
| run-event-loss-recovery | `test` | `passed` | yes | `cargo test -p orchest --lib events::` | 2026-09-21 | `git:self` | — | — | `EV-event-loss-recovery-test` |
| run-failure-escalation-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test failure_escalation` | 2026-07-31 | `git:self` | — | — | `EV-failure-escalation-test` |
| run-fixture-contract | `fixture` | `passed` | yes | `cargo test -p research-pipeline-demo --test findings_contract` | 2026-07-31 | `git:self` | — | — | `EV-fixture-contract-test` |
| run-live-provider | `live-provider` | `not-run` | yes | — | — | — | configured-by-env | configured-by-env | — |
| run-provider-fakes-verification | `test` | `passed` | yes | `cargo test -p orchest-provider --features testing fakes` | 2026-07-31 | `git:self` | — | — | `EV-provider-fakes-test` |
| run-report-smoke | `smoke` | `passed` | yes | `cargo run -p research-pipeline-demo --bin seam-report -- check --findings examples/demo/research-pipeline/findings.json --report docs/iteration/v0_11/seam-gap-analysis.md` | 2026-07-31 | `git:self` | — | — | `EV-report-smoke` |
| run-supervisor-watcher-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 2026-07-31 | `git:self` | — | — | `EV-child-control-demo-test`, `EV-supervisor-watcher-test` |
| run-watcher-arbitration-unit | `test` | `passed` | yes | `cargo test -p orchest --lib watcher_arbitration` | 2026-09-22 | `git:self` | — | — | `EV-watcher-action-arbitration-test` |
| run-watcher-order-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test watcher_order` | 2026-07-31 | `git:self` | — | — | `EV-watcher-order-test` |
| run-worker-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test worker` | 2026-07-31 | `git:self` | — | — | `EV-worker-test` |

#### run-child-control-unit

Orchest child_control unit tests prove RunHandle::child lookup, child-target inject/steer, preserved supervisor inject, and independent Completed/Failed wait_completion.

#### run-deterministic-runtime

The complete provider-independent demo package passed 69 tests across evidence contract, isolated report path resolution and read-only checking, worker, supervisor/watcher, terminal failure, and watcher ordering; the distinct credential-gated live-provider smoke remained ignored.

#### run-event-loss-recovery

Focused orchest events:: unit tests prove coalesced secondary EventsDropped with sequence metadata, flush-before-resume, O(1) pending state under sustained saturation, and unchanged no-drop FIFO.

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

Nine deterministic supervisor_watcher tests passed, including nested SubAgentEvent delivery, supervisor-only public steering paths, RunHandle::child resolve/await, and child-target inject/steer.

#### run-watcher-arbitration-unit

Orchest watcher_arbitration unit tests prove Abort precedence and registration-order Inject/Steer application independent of completion scheduling.

#### run-watcher-order-deterministic

Two supervisor watchers with capacity 1024 each observed the expected stable milestone subsequence and the same complete indexed sequence with no EventsDropped observed. The test returns no actions and makes no action-order claim.

#### run-worker-deterministic

Eight deterministic worker tests passed: corpus tools, fatal unsafe fault shape, threshold-one abort termination, Fresh and bounded Fork context, empty-parent error, and event rendering.

### Live-provider boundary

- **run-live-provider** — `not-run`; provider `configured-by-env`; model `configured-by-env`; The ignored tests/smoke.rs live-provider target was not executed because RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent. No waiver is granted; GitHub issue #258 owns the v1.0 live-verification gate.

## Evidence catalogue

| ID | Kind | Locator | Run | Command | Result | Summary |
| --- | --- | --- | --- | --- | --- | --- |
| EV-agent-as-tool-forwarding-source | `source` | `crates/orchest/src/tool/agent_as_tool.rs` · `AgentAsTool::run_child_attempt` | — | — | — | AgentAsTool forwards child RuntimeEvent values as SubAgentEvent through ToolContext::emit_event so attached watchers receive the nested stream; each child is registered on the shared ChildRunRegistry and publishes ChildRunOutcome for wait_completion. |
| EV-child-control-demo-test | `test` | `examples/demo/research-pipeline/tests/supervisor_watcher.rs` · `child_control_surface_targets_worker_and_awaits_completion` | run-supervisor-watcher-deterministic | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 9 passed; 0 failed; child control resolve/await + child-target inject/steer proven | Research Pipeline supervisor_watcher proves public resolve_delegated_worker_target / await_delegated_worker_completion and child_control_surface_targets_worker_and_awaits_completion for child-target inject/steer without altering supervisor histories. |
| EV-child-control-unit-test | `test` | `crates/orchest/src/tool/agent_as_tool.rs` · `child_control_targets_child_not_supervisor_and_completion_is_independent` | run-child-control-unit | `cargo test -p orchest --lib child_control` | 2 passed; 0 failed; child vs supervisor targeting and Failed outcome proven | Orchest lib tests prove RunHandle::child resolves after SubAgentStarted, child-target inject/steer change only the child conversation, supervisor inject remains supervisor-targeted, and wait_completion publishes Completed/Failed independently of the supervisor EventReceiver. |
| EV-controlled-fault | `source` | `examples/demo/research-pipeline/src/fault.rs` · `FaultTriggerTool::execute and ControlledFaultAbortHook::on_repeated_failure` | — | — | — | The controlled fault returns Fatal with Unsafe retry semantics, and the matching repeated-failure hook aborts the run. |
| EV-demo-package-test | `test` | `examples/demo/research-pipeline/Cargo.toml` | run-deterministic-runtime | `cargo test -p research-pipeline-demo` | 69 passed; 0 failed; 1 credential-gated live smoke ignored | The complete provider-independent Research Pipeline package passed without executing the ignored credential-gated provider smoke or writing the tracked canonical report. |
| EV-event-loss-recovery-test | `test` | `crates/orchest/src/events.rs` · `secondary_saturation_delivers_coalesced_loss_then_resumes` | run-event-loss-recovery | `cargo test -p orchest --lib events::` | 10 passed; 0 failed; saturation + bounded pending + no-drop FIFO | Deterministic saturation tests prove coalesced secondary loss signals with from_seq/to_seq, flush-before-resume after drain, O(1) pending state under sustained saturation, and unchanged no-drop FIFO. |
| EV-evidence-contract | `documentation` | `docs/archive/iteration/v0_11/finding-evidence-contract-design.md` · `Stable Seam Finding Evidence Design` | — | — | — | The evidence contract defines lifecycle, reference, and readiness requirements. |
| EV-failure-escalation-test | `test` | `examples/demo/research-pipeline/tests/failure_escalation.rs` · `controlled_worker_failure_reaches_supervisor_escalation_without_restart_or_panic` | run-failure-escalation-deterministic | `cargo test -p research-pipeline-demo --test failure_escalation` | 1 passed; 0 failed; ordered search-to-escalation chain; no RunRestarted captured | The focused target gates completion on the public supervisor EventReceiver terminal event, requires completed search_corpus before fault_trigger, and extracts strict indices for nested Fatal/Unsafe failure, threshold-one abort RunFailed, SubAgentFailed, parent SUB_AGENT_RUN_FAILED, and supervisor escalation. It separately proves configured restart absence and no hook panic. |
| EV-fixture-contract-test | `runtime-output` | `examples/demo/research-pipeline/tests/findings_contract.rs` · `canonical_final_executed_rows_use_the_containing_commit_revision` | run-fixture-contract | `cargo test -p research-pipeline-demo --test findings_contract` | 42 passed; 0 failed | The strict canonical contract fixture target passed, including the containing-commit provenance guard for final executed rows. |
| EV-known-seams | `documentation` | `docs/archive/iteration/v0_11/design-decisions.md` · `Pre-Identified Seam Gap Findings Summary` | — | — | — | Locked design decisions list the pre-identified seams and their stable ids. |
| EV-llm-watcher-builder-source | `source` | `crates/orchest/src/run/llm_watcher.rs` · `LlmWatcherBuilder::build` | — | — | — | LlmWatcherBuilder::build uses expect when no model was configured, causing a public library panic instead of a configuration error. |
| EV-llm-watcher-format-source | `source` | `crates/orchest/src/run/llm_watcher.rs` · `format_event` | — | — | — | LlmWatcher::format_event has structured arms for SubAgentStarted/Completed/Failed, SubAgentEvent, and ChildRunEvent; unit tests prove non-Debug formatting and supervisor_watcher proves runtime delivery. |
| EV-primary-tool-context-source | `source` | `crates/orchest/src/run/actor.rs` · `run_tool_and_handoff_phase` | — | — | — | The run actor constructs ToolContext with event_subs (full subscriber snapshot) and event_tx (primary); ToolContext::emit_event fans out tool-originated events to primary and attached watchers. |
| EV-provider-fakes-source | `source` | `crates/orchest-provider/src/fakes.rs` · `FakeAsr and FakeTts` | — | — | — | The orchest-provider wall exposes deterministic FakeAsr and FakeTts implementations behind the testing feature. |
| EV-provider-fakes-test | `test` | `crates/orchest-provider/src/fakes.rs` · `tests` | run-provider-fakes-verification | `cargo test -p orchest-provider --features testing fakes` | 5 passed; 0 failed; 7 filtered out | The focused provider-fakes test filter passed all five FakeAsr/FakeTts tests. |
| EV-public-api | `documentation` | `docs/archive/iteration/v0_11/implementation-plan.md` · `Public paths` | — | — | — | The v0.11 implementation overview fixes the public imports that the demo may use. |
| EV-report-smoke | `smoke-run` | `examples/demo/research-pipeline/src/bin/seam-report.rs` · `Command::Check` | run-report-smoke | `cargo run -p research-pipeline-demo --bin seam-report -- check --findings examples/demo/research-pipeline/findings.json --report docs/iteration/v0_11/seam-gap-analysis.md` | report current; byte-for-byte match | The seam-report staleness check passed against the deterministic Markdown projection. |
| EV-secondary-delivery-source | `source` | `crates/orchest/src/events.rs` · `deliver_to_subscribers` | — | — | — | Primary delivery awaits with timeout; secondaries use try_send with O(1) coalesced pending loss and EventsDropped recovery on the affected secondary (mirror to primary best-effort). |
| EV-supervisor-restart-source | `source` | `crates/orchest/src/run/supervisor.rs` · `SupervisorActor::handle_supervisor_evt` | — | — | — | The runtime emits RunRestarted only from SupervisorActor's ActorFailed branch; clean WorkerActor termination after RunFailed follows ActorTerminated instead. |
| EV-supervisor-source | `source` | `examples/demo/research-pipeline/src/supervisor.rs` · `build_supervisor and start_with_live_watchers` | — | — | — | The demo builds the worker through Worker::as_tool, registers it with an Orchest supervisor, instructs the fault path to call search_corpus before fault_trigger and then return an escalation summary without retrying, exposes only the supervisor RunHandle, and starts with AgentRun::start_with_watchers so declared watchers observe from RunStarted. |
| EV-supervisor-watcher-test | `test` | `examples/demo/research-pipeline/tests/supervisor_watcher.rs` · `activated_watchers_prove_nested_routing_and_applied_supervisor_actions` | run-supervisor-watcher-deterministic | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 9 passed; 0 failed; nested SubAgentEvent + child control resolve/await + child-target inject/steer | The focused target gates the first supervisor call, releases a harmless probe so queued subscriptions activate, and releases second-step delegation only after both watcher wrappers complete its ModelCallStarted event. It proves each public supervisor steering path changes only supervisor history, observes forwarded child completion on the primary EventReceiver and on both attached watchers, resolves RunHandle::child after SubAgentStarted, awaits child completion independently, and includes a dedicated child-target inject/steer proof. |
| EV-watcher-action-arbitration-test | `test` | `crates/orchest/src/run/tests.rs` · `watcher_arbitration_abort_wins_independent_of_completion_order` | run-watcher-arbitration-unit | `cargo test -p orchest --lib watcher_arbitration` | watcher_arbitration tests passed; schedule-independent Abort and registration-order Inject/Steer | Adversarial gated ActionArbitrator tests prove Abort wins regardless of completion order and Inject/Steer apply in registration order; public arbitrate_watcher_actions documents precedence. |
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

- SB-3 — Restart does not cover run-level failure (`open`, #251)

### Post-1.0 backlog

- P1-1 — LlmWatcher is not root re-exported (`deferred`, #256)
- P1-2 — ContextMode is not root re-exported (`deferred`, #256)
- P1-3 — Fork empty-parent context error is unreachable (`deferred`, #257)
- P1-4 — Delegation has no explicit child completion receiver (`verified`, #249)
- P1-5 — Provider test fakes were previously inaccessible (`verified`, #196)
