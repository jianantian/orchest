# Prime Agent Lessons: Orchest Adoption Map

> **Purpose:** turn the comparison into bounded decisions without expanding
> Orchest core into a coding-agent product.

## Classification

- **Adopt:** the semantic contract is general and fits an existing Orchest
  boundary.
- **Adapt:** the idea is valuable but Prime Agent's ownership or API shape does
  not fit Orchest.
- **Reject:** copying it would violate an Orchest boundary or weaken a current
  guarantee.
- **Defer:** evidence is insufficient or the capability belongs after the v1.0
  public API gates.

## Decision Map

| Prime Agent lesson | Decision | Target layer | Reason |
| --- | --- | --- | --- |
| Separate child admission from completion | Adopt | Orchest core | General multi-agent lifecycle primitive |
| Stable retained-child handle | Adopt | Orchest core | Required for public child steering, observation, and completion |
| Parent-scoped run-tree registry | Adapt | Core seam + Code Agent persistence | Core owns identity; product owns residency and retention duration |
| Explicit agent-to-agent messages | Defer | Orchest core | Requires stable child control and admission semantics first |
| Fresh child context by default | Adopt | Orchest core | Already aligned with `ContextMode::Fresh` |
| Fail rather than silently downgrade Fork/model selection | Adopt | Orchest core | Already aligned with Orchest's explicit error policy |
| Child usage attribution | Adopt | Orchest core | Orchest already propagates child budget; retained runs must preserve it |
| Unified input admission | Adapt | Orchest core | Define generic envelopes and ordering, not product-specific sources |
| Heartbeats and cron schedules | Adapt | Code Agent service | Consume input admission; do not enter SDK loop policy |
| Persistent goals | Adapt | Code Agent harness | Goal policy is product behavior; budget signals remain core |
| Autonomous continuation | Adapt | Code Agent harness | Use explicit budgets and quality gates; never equate limit with success |
| Generation-aware event cursors | Adapt | Core event seam + service protocol | Core needs identity/loss; service owns transport generations |
| Snapshot fallback after replay gaps | Adopt | Core contract + service implementation | Durable state baseline is safer than infinite replay |
| Command mutation journal | Adapt | Code Agent service | Needed for side effects, but daemon-specific storage is not SDK core |
| Worker per root session tree | Defer | Code Agent service | Useful topology; validate operational needs before fixing process shape |
| Separate catalog process | Defer | Code Agent service | Optimization justified only by real scan/load contention |
| Persistent IPython namespace | Adapt | Code Agent runtime | Valuable optional execution capability, not universal Tool model |
| Typed kernel-to-host requests | Adopt | Code Agent runtime | Keeps runtime truth and credentials outside model-generated code |
| Kernel snapshot/restore | Defer | Code Agent runtime/service | Valuable but backend- and security-sensitive |
| Continual harness ledger | Adapt | Code Agent harness | Keep typed, scoped, reviewable, and eval-gated |
| Base prompt remains immutable | Adopt | Code Agent harness | Prevents invisible policy replacement |
| Session-local refinement default | Adopt | Code Agent harness | Limits blast radius and supports rollback |
| Python-backed Skill as Skill superset | Reject | — | Violates Orchest Tool/MCP/Skill boundary |
| IPython as the only built-in capability | Reject | — | Coding-specific preference must not replace the Tool registry |
| User-permission process treated as sufficient containment | Reject | — | Weaker than Orchest's injectable executor and safety direction |
| Product daemon protocol as public gateway | Reject | — | Internal local transport is not a stable hosted contract |
| Default-on product telemetry in SDK runtime | Reject | — | Collection policy belongs to the embedding product |

## Delivery Order

### Phase 0: Finish existing v1.0 correctness gates

Do not open a parallel Prime-inspired core redesign. The current v1.0 issues
already cover the foundation:

| Existing issue | Prime lesson it unlocks |
| --- | --- |
| 001 delegated child control | retained child handles and child completion |
| 002 nested watcher events | observable run trees |
| 003 run-level restart | failure-aware recovery |
| 004 watcher event loss | replay/resynchronization boundary |
| 005 pre-run watcher attachment | deterministic first-event observation |
| 006 watcher action arbitration | ordered control admission |
| 007 fallible watcher builder | public API error hygiene |
| 008 live-provider verification | evidence before API freeze |

The Prime Agent research should refine the semantics of these fixes, not expand
the v1.0 release scope.

### Phase 1: Retained delegation foundation

After the public child-control gate:

1. preserve `AgentAsTool` as synchronous delegation;
2. introduce asynchronous child admission;
3. add public status, completion, event, abort, inject, and steer operations;
4. make parent-child edges inspectable;
5. persist optional run-tree state through a narrow store seam; and
6. prove budget and approval behavior after the parent turn ends.

### Phase 2: Orchest Code Agent execution foundation

Build outside core:

1. persistent `ExecutionSession` abstraction;
2. one backend with explicit containment capabilities;
3. typed host bridge over existing runtime/Tool contracts;
4. append-only trajectory and artifact ledger;
5. restart and restoration semantics; and
6. workspace-safe file, shell, edit, search, and git capabilities.

### Phase 3: Long-running service

Only after retained runs and execution sessions have stable identity:

1. worker residency and passivation;
2. input admission from user, agents, and schedulers;
3. command IDs and mutation journal;
4. event cursors and snapshot resynchronization;
5. heartbeat and cron producers; and
6. orphan and crash recovery.

### Phase 4: Continual harness

Add after trajectory capture and evaluation are trustworthy:

1. typed session-local harness entries;
2. refinement proposals with evidence references;
3. before/after revisions and rollback;
4. evaluation-based acceptance;
5. explicit project/global promotion; and
6. no executable-authority expansion through refinement.

## Architecture Guardrail

```text
                         ┌─────────────────────────────┐
                         │ Code Agent service          │
                         │ workers · schedules · replay│
                         └──────────────┬──────────────┘
                                        │ uses
                         ┌──────────────▼──────────────┐
                         │ Code Agent runtime          │
                         │ execution · workspace       │
                         │ artifacts · harness         │
                         └──────────────┬──────────────┘
                                        │ embeds
                         ┌──────────────▼──────────────┐
                         │ Orchest core                │
                         │ loop · events · control     │
                         │ tools · budgets · approval  │
                         └─────────────────────────────┘
```

No upper-layer feature becomes a core requirement merely because Prime Agent
implements it in the same package.

## Decision Tests

Before adopting another Prime Agent mechanism, ask:

1. Is it needed by non-coding agent applications?
2. Does it preserve provider neutrality?
3. Can it be expressed without daemon, filesystem, or UI types?
4. Does it strengthen rather than bypass Tool, approval, budget, and Skill
   boundaries?
5. Can the behavior be verified independently of Prime Agent?
6. Does it have a smaller extension seam than a new core feature?

If the answer to 1–3 is no, the default owner is the Code Agent runtime or
service. If the answer to 4 or 5 is no, reject or defer it.

## Related Research

- [Runtime lessons](./orchest-vs-prime-agent-runtime-lessons.md)
- [RLM and retained agents](./prime-agent-rlm-and-retained-agents.md)
- [Long-running runtime](./prime-agent-long-running-runtime.md)
- [Persistent execution and continual harness](./prime-agent-persistent-execution-and-harness.md)
