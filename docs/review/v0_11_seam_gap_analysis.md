# Research Pipeline seam gap analysis

## Executive summary

Deterministic Research Pipeline seam checks passed. One release blocker (RB-1/#255) remains open; the required live-provider run is not-run (#258).

## Readiness verdict

**Status:** `unverified`

**Reason:** Required live-provider run is not-run: RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent; #258 still blocks v1.0.

**References:** `run-live-provider`

## Seam API checklist

| ID | API surface | Public path | Requirement | Status | Evidence | Findings |
| --- | --- | --- | --- | --- | --- | --- |
| API-1 | Watcher attachment and external steering | `orchest::run::RunHandle` | Attach watchers and exercise external steering through the supervisor handle. | `exercised` | `EV-public-api`, `EV-supervisor-source`, `EV-supervisor-watcher-test` | `SB-1`, `SB-2`, `SB-6` |
| API-2 | Supervisor event observation | `orchest::run::EventReceiver` | Observe terminal supervisor events without a fixed timeout. | `exercised` | `EV-failure-escalation-test`, `EV-public-api`, `EV-supervisor-watcher-test` | `P1-4` |
| API-3 | Worker restart policy | `orchest::run::SupervisionStrategy` | Configure restart and record whether a run-level failure restarts the delegated worker. | `exercised` | `EV-failure-escalation-test`, `EV-public-api`, `EV-run-level-restart-unit-test`, `EV-supervisor-restart-source` | `SB-3` |
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
| SB-3 | Restart does not cover run-level failure | `seam-blocker` | `verified` | #251 |
| SB-4 | LlmWatcher does not format nested delegation events | `seam-blocker` | `verified` | #250 |
| SB-5 | Secondary watcher subscribers can drop events | `seam-blocker` | `verified` | #252 |
| SB-6 | No public start-with-watchers or pre-run pause seam | `seam-blocker` | `verified` | #253 |
| SB-7 | Watcher actions have no global registration-order guarantee | `seam-blocker` | `verified` | #254 |
| SB-8 | Forwarded child events bypass attached watchers | `seam-blocker` | `verified` | #250 |

### P1-1 — LlmWatcher is not root re-exported

**API surface:** `orchest::run::llm_watcher::LlmWatcher`

**Classification:** `post-1.0-backlog`

**Status:** `deferred`

**Description:** LlmWatcher requires a deeper import than sibling watcher APIs.

**Observed consequence:** Public discoverability is weaker but the path remains usable.

**Workaround:** Import via orchest::run::llm_watcher::LlmWatcher.

**Evidence:** `EV-known-seams`, `EV-public-api`

**Action owner:** orchest-maintainers

**Action:** Add a convenience re-export after v1.0 (#256).

**Issue:** #256

**Verification status:** `not-applicable`

**Verification summary:** Deferred to post-1.0 backlog under #256.

**Verification commands:** —

**Verification evidence:** `EV-public-api`

### P1-2 — ContextMode is not root re-exported

**API surface:** `orchest::tool::agent_as_tool::ContextMode`

**Classification:** `post-1.0-backlog`

**Status:** `deferred`

**Description:** ContextMode is not root-re-exported.

**Observed consequence:** Public discoverability is weaker but the path remains usable.

**Workaround:** Import via orchest::tool::agent_as_tool::ContextMode.

**Evidence:** `EV-known-seams`, `EV-public-api`

**Action owner:** orchest-maintainers

**Action:** Add a convenience re-export after v1.0 (#256).

**Issue:** #256

**Verification status:** `not-applicable`

**Verification summary:** Deferred to post-1.0 backlog under #256.

**Verification commands:** —

**Verification evidence:** `EV-public-api`

### P1-3 — Fork empty-parent context error is unreachable

**API surface:** `orchest::tool::agent_as_tool::ContextMode::Fork`

**Classification:** `post-1.0-backlog`

**Status:** `deferred`

**Description:** Fork empty-parent context error path is unreachable in practice.

**Observed consequence:** Error taxonomy claims a case callers cannot hit.

**Workaround:** Document the unreachable path; tracked by #257.

**Evidence:** `EV-known-seams`, `EV-worker-test`

**Action owner:** orchest-maintainers

**Action:** Clarify or remove the unreachable Fork empty-parent error (#257).

**Issue:** #257

**Verification status:** `passed`

**Verification summary:** Deferred to post-1.0 backlog under #257.

**Verification commands:** `cargo test -p research-pipeline-demo --test worker`

**Verification evidence:** `EV-worker-test`

### P1-4 — Delegation has no explicit child completion receiver

**API surface:** `orchest::run::EventReceiver`

**Classification:** `post-1.0-backlog`

**Status:** `verified`

**Description:** Delegation lacked an explicit child completion receiver.

**Observed consequence:** Callers waited only on supervisor completion, not the child.

**Workaround:** Resolved via child wait_completion on ChildRunHandle (#249).

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-failure-escalation-test`, `EV-known-seams`

**Action owner:** orchest-maintainers

**Action:** Expose independent child completion wait on ChildRunHandle.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** Child await proofs in supervisor_watcher passed.

**Verification commands:** `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test failure_escalation`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-failure-escalation-test`

### P1-5 — Provider test fakes were previously inaccessible

**API surface:** `orchest-provider::fakes`

**Classification:** `post-1.0-backlog`

**Status:** `verified`

**Description:** Provider test fakes were previously inaccessible to consumers.

**Observed consequence:** Demos and tests could not reuse shared provider fakes.

**Workaround:** Resolved by exporting provider fakes (#196).

**Evidence:** `EV-known-seams`, `EV-provider-fakes-source`, `EV-provider-fakes-test`

**Action owner:** orchest-maintainers

**Action:** Publish shared provider fakes for deterministic tests.

**Issue:** #196

**Action revision:** `8bb9a9b`

**Verification status:** `passed`

**Verification summary:** Provider fakes unit verification passed.

**Verification commands:** `cargo test -p orchest-provider --features testing fakes`

**Verification evidence:** `EV-provider-fakes-test`

### RB-1 — LlmWatcher builder panics without a model

**API surface:** `orchest::run::llm_watcher::LlmWatcherBuilder`

**Classification:** `release-blocker`

**Status:** `open`

**Description:** LlmWatcher builder panics when no model is configured.

**Observed consequence:** Missing-model construction fails hard instead of a typed error.

**Workaround:** Always supply a model before build; tracked by #255.

**Evidence:** `EV-known-seams`, `EV-llm-watcher-builder-source`

**Action owner:** orchest-maintainers

**Action:** Return a typed builder error instead of panicking.

**Issue:** #255

**Verification status:** `not-run`

**Verification summary:** Not run; release blocker remains open under #255.

**Verification commands:** —

**Verification evidence:** `EV-llm-watcher-builder-source`

### SB-1 — Delegation does not expose a child RunHandle

**API surface:** `orchest::tool::agent_as_tool::AgentAsTool`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Delegation lacked a public child RunHandle for inject/steer/wait.

**Observed consequence:** Callers could not target delegated workers through the public API.

**Workaround:** Resolved via ChildRunHandle / ChildRunRegistry (#249).

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-known-seams`, `EV-supervisor-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Expose child control through ChildRunHandle after SubAgentStarted.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** Child resolve/await and child-target inject/steer proofs passed.

**Verification commands:** `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test`

### SB-2 — Steering targets the supervisor rather than delegated worker

**API surface:** `orchest::run::WatcherAction and orchest::run::RunHandle`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Steering applied to the supervisor instead of the delegated worker.

**Observed consequence:** Worker-targeted inject/steer could not be expressed publicly.

**Workaround:** Resolved via child-targeted control on ChildRunHandle (#249).

**Evidence:** `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-known-seams`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing`

**Action owner:** orchest-maintainers

**Action:** Route inject/steer to the child run when a child handle is resolved.

**Issue:** #249

**Verification status:** `passed`

**Verification summary:** Child-target inject/steer deterministic proofs passed.

**Verification commands:** `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing`

### SB-3 — Restart does not cover run-level failure

**API surface:** `orchest::run::SupervisionStrategy`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Restart covered actor crashes but not eligible run-level RunFailed.

**Observed consequence:** Delegated workers could not recover from eligible tool/budget failures.

**Workaround:** Resolved via bounded Restart on eligible RunFailed (#251).

**Evidence:** `EV-failure-escalation-test`, `EV-known-seams`, `EV-run-level-restart-unit-test`, `EV-supervisor-restart-source`, `EV-worker-source`

**Action owner:** orchest-maintainers

**Action:** Apply Restart to eligible Other RunFailed; keep budget/max-steps terminal.

**Issue:** #251

**Verification status:** `passed`

**Verification summary:** failure_escalation + run-level restart unit proofs passed.

**Verification commands:** `cargo test -p orchest --lib run_failed_; cargo test -p orchest --lib restart_`<br>`cargo test -p research-pipeline-demo --test failure_escalation`

**Verification evidence:** `EV-failure-escalation-test`, `EV-run-level-restart-unit-test`, `EV-supervisor-restart-source`

### SB-4 — LlmWatcher does not format nested delegation events

**API surface:** `orchest::run::llm_watcher::LlmWatcher`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** LlmWatcher did not format nested delegation events.

**Observed consequence:** Nested SubAgentEvent content was opaque to LLM watchers.

**Workaround:** Resolved via structured nested formatting (#250).

**Evidence:** `EV-known-seams`, `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Format nested delegation events in LlmWatcher output.

**Issue:** #250

**Verification status:** `passed`

**Verification summary:** Supervisor/watcher nested-format proofs passed.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test`

### SB-5 — Secondary watcher subscribers can drop events

**API surface:** `orchest::events::EventSink / orchest::events::RuntimeEvent::EventsDropped`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Secondary watcher subscribers could drop events under load.

**Observed consequence:** Non-primary watchers might miss coalesced delivery.

**Workaround:** Resolved via EventSink coalesced loss recovery (#252).

**Evidence:** `EV-event-loss-recovery-test`, `EV-known-seams`, `EV-secondary-delivery-source`, `EV-watcher-order-test`

**Action owner:** orchest-maintainers

**Action:** Recover coalesced secondary-subscriber loss without dropping identity.

**Issue:** #252

**Verification status:** `passed`

**Verification summary:** Event-loss recovery proofs passed.

**Verification commands:** `cargo test -p orchest --lib events::`<br>`cargo test -p research-pipeline-demo --test watcher_order`

**Verification evidence:** `EV-event-loss-recovery-test`, `EV-secondary-delivery-source`, `EV-watcher-order-test`

### SB-6 — No public start-with-watchers or pre-run pause seam

**API surface:** `orchest::run::AgentRun::start_with_watchers`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** No public start-with-watchers or pre-run pause seam.

**Observed consequence:** Watchers could miss RunStarted before the first model call.

**Workaround:** Resolved via AgentRun::start_with_watchers (#253).

**Evidence:** `EV-known-seams`, `EV-supervisor-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Attach declared watchers before the first model call.

**Issue:** #253

**Verification status:** `passed`

**Verification summary:** start_with_watchers / watcher-order proofs passed.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-supervisor-source`, `EV-supervisor-watcher-test`

### SB-7 — Watcher actions have no global registration-order guarantee

**API surface:** `orchest::run::WatcherAction`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Watcher actions lacked a global registration-order guarantee.

**Observed consequence:** Competing watcher actions had unspecified arbitration.

**Workaround:** Resolved via ActionArbitrator registration-order behavior (#254).

**Evidence:** `EV-known-seams`, `EV-watcher-action-arbitration-test`, `EV-watcher-order-test`, `EV-watcher-task-topology`

**Action owner:** orchest-maintainers

**Action:** Arbitrate competing watcher actions in registration order.

**Issue:** #254

**Verification status:** `passed`

**Verification summary:** Watcher arbitration unit + order deterministic proofs passed.

**Verification commands:** `cargo test -p orchest --lib watcher_arbitration`<br>`cargo test -p research-pipeline-demo --test watcher_order`

**Verification evidence:** `EV-watcher-action-arbitration-test`, `EV-watcher-order-test`, `EV-watcher-task-topology`

### SB-8 — Forwarded child events bypass attached watchers

**API surface:** `orchest::run::EventReceiver and orchest::run::RunHandle::attach_watcher`

**Classification:** `seam-blocker`

**Status:** `verified`

**Description:** Forwarded child events bypassed attached watchers.

**Observed consequence:** Attached watchers could not observe nested child traffic.

**Workaround:** Resolved via ToolContext::emit_event fan-out (#250).

**Evidence:** `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test`

**Action owner:** orchest-maintainers

**Action:** Fan out forwarded child events to attached watchers.

**Issue:** #250

**Verification status:** `passed`

**Verification summary:** Nested routing / fan-out deterministic proofs passed.

**Verification commands:** `cargo test -p research-pipeline-demo --test supervisor_watcher`

**Verification evidence:** `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test`

## Verification evidence

| Finding | Status | Commands | Evidence | Summary |
| --- | --- | --- | --- | --- |
| P1-1 | `not-applicable` | — | `EV-public-api` | Deferred to post-1.0 backlog under #256. |
| P1-2 | `not-applicable` | — | `EV-public-api` | Deferred to post-1.0 backlog under #256. |
| P1-3 | `passed` | `cargo test -p research-pipeline-demo --test worker` | `EV-worker-test` | Deferred to post-1.0 backlog under #257. |
| P1-4 | `passed` | `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test failure_escalation`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-failure-escalation-test` | Child await proofs in supervisor_watcher passed. |
| P1-5 | `passed` | `cargo test -p orchest-provider --features testing fakes` | `EV-provider-fakes-test` | Provider fakes unit verification passed. |
| RB-1 | `not-run` | — | `EV-llm-watcher-builder-source` | Not run; release blocker remains open under #255. |
| SB-1 | `passed` | `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test` | Child resolve/await and child-target inject/steer proofs passed. |
| SB-2 | `passed` | `cargo test -p orchest --lib child_control`<br>`cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-child-control-demo-test`, `EV-child-control-unit-test`, `EV-supervisor-watcher-test`, `EV-watcher-action-routing` | Child-target inject/steer deterministic proofs passed. |
| SB-3 | `passed` | `cargo test -p orchest --lib run_failed_; cargo test -p orchest --lib restart_`<br>`cargo test -p research-pipeline-demo --test failure_escalation` | `EV-failure-escalation-test`, `EV-run-level-restart-unit-test`, `EV-supervisor-restart-source` | failure_escalation + run-level restart unit proofs passed. |
| SB-4 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-llm-watcher-format-source`, `EV-supervisor-watcher-test` | Supervisor/watcher nested-format proofs passed. |
| SB-5 | `passed` | `cargo test -p orchest --lib events::`<br>`cargo test -p research-pipeline-demo --test watcher_order` | `EV-event-loss-recovery-test`, `EV-secondary-delivery-source`, `EV-watcher-order-test` | Event-loss recovery proofs passed. |
| SB-6 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-supervisor-source`, `EV-supervisor-watcher-test` | start_with_watchers / watcher-order proofs passed. |
| SB-7 | `passed` | `cargo test -p orchest --lib watcher_arbitration`<br>`cargo test -p research-pipeline-demo --test watcher_order` | `EV-watcher-action-arbitration-test`, `EV-watcher-order-test`, `EV-watcher-task-topology` | Watcher arbitration unit + order deterministic proofs passed. |
| SB-8 | `passed` | `cargo test -p research-pipeline-demo --test supervisor_watcher` | `EV-agent-as-tool-forwarding-source`, `EV-primary-tool-context-source`, `EV-supervisor-watcher-test` | Nested routing / fan-out deterministic proofs passed. |

## Run evidence

Revision `git:self` denotes the commit containing the canonical findings file and is reserved for post-commit verification evidence.

| ID | Kind | Status | Required | Command | Date | Revision | Provider | Model | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| run-child-control-unit | `test` | `passed` | yes | `cargo test -p orchest --lib child_control` | 2026-09-22 | `git:self` | — | — | `EV-child-control-unit-test` |
| run-deterministic-runtime | `test` | `passed` | yes | `cargo test -p research-pipeline-demo` | 2026-07-31 | `git:self` | — | — | `EV-demo-package-test` |
| run-event-loss-recovery | `test` | `passed` | yes | `cargo test -p orchest --lib events::` | 2026-09-21 | `git:self` | — | — | `EV-event-loss-recovery-test` |
| run-failure-escalation-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test failure_escalation` | 2026-09-22 | `git:self` | — | — | `EV-failure-escalation-test` |
| run-fixture-contract | `fixture` | `passed` | yes | `cargo test -p research-pipeline-demo --test findings_contract` | 2026-07-31 | `git:self` | — | — | `EV-fixture-contract-test` |
| run-level-restart-unit | `test` | `passed` | yes | `cargo test -p orchest --lib run_failed_; cargo test -p orchest --lib restart_` | 2026-09-22 | `git:self` | — | — | `EV-run-level-restart-unit-test` |
| run-live-provider | `live-provider` | `not-run` | yes | — | — | — | configured-by-env | configured-by-env | — |
| run-provider-fakes-verification | `test` | `passed` | yes | `cargo test -p orchest-provider --features testing fakes` | 2026-07-31 | `git:self` | — | — | `EV-provider-fakes-test` |
| run-report-smoke | `smoke` | `passed` | yes | `cargo run -p research-pipeline-demo --bin seam-report -- check --findings examples/demo/research-pipeline/findings.json --report docs/iteration/v0_11/seam-gap-analysis.md` | 2026-07-31 | `git:self` | — | — | `EV-report-smoke` |
| run-supervisor-watcher-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 2026-07-31 | `git:self` | — | — | `EV-child-control-demo-test`, `EV-supervisor-watcher-test` |
| run-watcher-arbitration-unit | `test` | `passed` | yes | `cargo test -p orchest --lib watcher_arbitration` | 2026-09-22 | `git:self` | — | — | `EV-watcher-action-arbitration-test` |
| run-watcher-order-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test watcher_order` | 2026-07-31 | `git:self` | — | — | `EV-watcher-order-test` |
| run-worker-deterministic | `test` | `passed` | yes | `cargo test -p research-pipeline-demo --test worker` | 2026-07-31 | `git:self` | — | — | `EV-worker-test` |

#### run-child-control-unit

ChildRunHandle unit proofs passed.

#### run-deterministic-runtime

Provider-independent research-pipeline-demo package tests passed; live smoke ignored.

#### run-event-loss-recovery

EventSink coalesced loss-recovery proofs passed.

#### run-failure-escalation-deterministic

Failure escalation / restart deterministic proofs passed.

#### run-fixture-contract

findings_contract passed, including canonical git:self provenance.

#### run-level-restart-unit

Run-level Restart unit proofs passed.

#### run-live-provider

Not run: RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY absent.

#### run-provider-fakes-verification

Provider fakes verification passed.

#### run-report-smoke

seam-report check matched the canonical Markdown projection.

#### run-supervisor-watcher-deterministic

Supervisor/watcher nested routing and child-control proofs passed.

#### run-watcher-arbitration-unit

ActionArbitrator unit proofs passed.

#### run-watcher-order-deterministic

Watcher registration-order deterministic proofs passed.

#### run-worker-deterministic

Deterministic worker tests passed.

### Live-provider boundary

- **run-live-provider** — `not-run`; provider `configured-by-env`; model `configured-by-env`; Not run: RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY absent.

## Evidence catalogue

| ID | Kind | Locator | Run | Command | Result | Summary |
| --- | --- | --- | --- | --- | --- | --- |
| EV-agent-as-tool-forwarding-source | `source` | `crates/orchest/src/tool/agent_as_tool.rs` · `AgentAsTool::run_child_attempt` | — | — | — | Nested SubAgentEvent forwarding. |
| EV-child-control-demo-test | `test` | `examples/demo/research-pipeline/tests/supervisor_watcher.rs` · `child_control_surface_targets_worker_and_awaits_completion` | run-supervisor-watcher-deterministic | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 9 passed | Demo child resolve/await / inject/steer proofs. |
| EV-child-control-unit-test | `test` | `crates/orchest/src/tool/agent_as_tool.rs` · `child_control_targets_child_not_supervisor_and_completion_is_independent` | run-child-control-unit | `cargo test -p orchest --lib child_control` | 2 passed; 0 failed; child vs supervisor targeting and Failed outcome proven | ChildRunHandle unit proofs. |
| EV-controlled-fault | `source` | `examples/demo/research-pipeline/src/fault.rs` · `FaultTriggerTool::execute and ControlledFaultAbortHook::on_repeated_failure` | — | — | — | Controlled fault injection for failure escalation. |
| EV-demo-package-test | `test` | `examples/demo/research-pipeline/Cargo.toml` | run-deterministic-runtime | `cargo test -p research-pipeline-demo` | 69 passed; 0 failed; 1 credential-gated live smoke ignored | Full provider-independent demo package tests. |
| EV-event-loss-recovery-test | `test` | `crates/orchest/src/events.rs` · `secondary_saturation_delivers_coalesced_loss_then_resumes` | run-event-loss-recovery | `cargo test -p orchest --lib events::` | 10 passed; 0 failed; saturation + bounded pending + no-drop FIFO | Coalesced secondary-subscriber loss recovery. |
| EV-evidence-contract | `documentation` | `docs/archive/iteration/v0_11/finding-evidence-contract-design.md` · `Stable Seam Finding Evidence Design` | — | — | — | Finding/evidence contract for canonical findings.json. |
| EV-failure-escalation-test | `test` | `examples/demo/research-pipeline/tests/failure_escalation.rs` · `controlled_worker_failure_restarts_once_then_escalates_without_panic` | run-failure-escalation-deterministic | `cargo test -p research-pipeline-demo --test failure_escalation` | 1 passed | Terminal failure / restart escalation proofs. |
| EV-fixture-contract-test | `runtime-output` | `examples/demo/research-pipeline/tests/findings_contract.rs` · `canonical_final_executed_rows_use_the_containing_commit_revision` | run-fixture-contract | `cargo test -p research-pipeline-demo --test findings_contract` | 42 passed; 0 failed | findings_contract suite including git:self provenance. |
| EV-known-seams | `documentation` | `docs/archive/iteration/v0_11/design-decisions.md` · `Pre-Identified Seam Gap Findings Summary` | — | — | — | Documented supervised-delegation seam inventory. |
| EV-llm-watcher-builder-source | `source` | `crates/orchest/src/run/llm_watcher.rs` · `LlmWatcherBuilder::build` | — | — | — | LlmWatcher builder missing-model seam. |
| EV-llm-watcher-format-source | `source` | `crates/orchest/src/run/llm_watcher.rs` · `format_event` | — | — | — | LlmWatcher nested event formatting. |
| EV-primary-tool-context-source | `source` | `crates/orchest/src/run/actor.rs` · `run_tool_and_handoff_phase` | — | — | — | ToolContext emit_event fan-out path. |
| EV-provider-fakes-source | `source` | `crates/orchest-provider/src/fakes.rs` · `FakeAsr and FakeTts` | — | — | — | Shared provider fakes export. |
| EV-provider-fakes-test | `test` | `crates/orchest-provider/src/fakes.rs` · `tests` | run-provider-fakes-verification | `cargo test -p orchest-provider --features testing fakes` | 5 passed; 0 failed; 7 filtered out | Provider fakes unit tests. |
| EV-public-api | `documentation` | `docs/archive/iteration/v0_11/implementation-plan.md` · `Public paths` | — | — | — | Public API boundary for the Research Pipeline demo. |
| EV-report-smoke | `smoke-run` | `examples/demo/research-pipeline/src/bin/seam-report.rs` · `Command::Check` | run-report-smoke | `cargo run -p research-pipeline-demo --bin seam-report -- check --findings examples/demo/research-pipeline/findings.json --report docs/iteration/v0_11/seam-gap-analysis.md` | report current; byte-for-byte match | seam-report check against canonical Markdown projection. |
| EV-run-level-restart-unit-test | `test` | `crates/orchest/src/run/tests.rs` · `run_failed_from_tool_hook_restarts_and_succeeds` | run-level-restart-unit | `cargo test -p orchest --lib run_failed_; cargo test -p orchest --lib restart_` | 4 focused restart-policy tests passed; actor-crash restart suite still passes | Run-level Restart success/exhaustion/terminal cases. |
| EV-secondary-delivery-source | `source` | `crates/orchest/src/events.rs` · `deliver_to_subscribers` | — | — | — | Secondary EventSink delivery / loss recovery. |
| EV-supervisor-restart-source | `source` | `crates/orchest/src/run/supervisor.rs` · `SupervisorActor::handle_supervisor_evt` | — | — | — | Restart policy on eligible RunFailed. |
| EV-supervisor-source | `source` | `examples/demo/research-pipeline/src/supervisor.rs` · `build_supervisor and start_with_live_watchers` | — | — | — | Supervisor wiring and start_with_watchers usage. |
| EV-supervisor-watcher-test | `test` | `examples/demo/research-pipeline/tests/supervisor_watcher.rs` · `activated_watchers_prove_nested_routing_and_applied_supervisor_actions` | run-supervisor-watcher-deterministic | `cargo test -p research-pipeline-demo --test supervisor_watcher` | 9 passed | Nested routing, child control, supervisor steering proofs. |
| EV-watcher-action-arbitration-test | `test` | `crates/orchest/src/run/tests.rs` · `watcher_arbitration_abort_wins_independent_of_completion_order` | run-watcher-arbitration-unit | `cargo test -p orchest --lib watcher_arbitration` | watcher_arbitration tests passed | ActionArbitrator registration-order unit proofs. |
| EV-watcher-action-routing | `source` | `crates/orchest/src/run/supervisor.rs` · `reattach_watcher` | — | — | — | Runtime watcher action routing. |
| EV-watcher-order-test | `test` | `examples/demo/research-pipeline/tests/watcher_order.rs` · `two_watchers_preserve_fifo_and_match_the_complete_no_drop_sequence` | run-watcher-order-deterministic | `cargo test -p research-pipeline-demo --test watcher_order` | 1 passed; 0 failed; sequences equal; no action-order inference | Watcher registration-order deterministic proofs. |
| EV-watcher-source | `source` | `examples/demo/research-pipeline/src/watcher.rs` · `stable_event_key and CountingWatcher::on_event and RecordingActionWatcher::on_event and RecordingLlmWatcher::on_event` | — | — | — | Demo watcher wrappers and action routing. |
| EV-watcher-task-topology | `source` | `crates/orchest/src/run/supervisor.rs` · `reattach_watcher` | — | — | — | Watcher task topology and registration order. |
| EV-worker-events | `source` | `examples/demo/research-pipeline/src/events.rs` · `render_event` | — | — | — | Worker-facing runtime event helpers. |
| EV-worker-source | `source` | `examples/demo/research-pipeline/src/worker.rs` · `Worker::from_paths` | — | — | — | Worker agent built through SubAgentBuilder. |
| EV-worker-test | `test` | `examples/demo/research-pipeline/tests/worker.rs` · `worker_threshold_and_abort_hook_produce_terminal_run_failed` | run-worker-deterministic | `cargo test -p research-pipeline-demo --test worker` | 8 passed; 0 failed; P1-3 exercised through public Tool::call_oneshot | Deterministic worker package tests. |

## v1.0 and Multivac M2 implications

### v1.0

Readiness remains `unverified`: Required live-provider run is not-run: RESEARCH_PIPELINE_CHAT_MODEL and RESEARCH_PIPELINE_API_KEY are absent; #258 still blocks v1.0.

Required live-provider gates: `run-live-provider`.

Unresolved release blockers:

- RB-1 — LlmWatcher builder panics without a model (`open`, #255)

### Multivac M2

No unresolved seam blockers are recorded.

### Post-1.0 backlog

- P1-1 — LlmWatcher is not root re-exported (`deferred`, #256)
- P1-2 — ContextMode is not root re-exported (`deferred`, #256)
- P1-3 — Fork empty-parent context error is unreachable (`deferred`, #257)
- P1-4 — Delegation has no explicit child completion receiver (`verified`, #249)
- P1-5 — Provider test fakes were previously inaccessible (`verified`, #196)
