# Prime Agent Runtime Lessons Research Design

**Date:** 2026-08-11

**Orchest baseline:** `834b697`

**Prime Agent baseline:** `14d6e7491` (`v0.7.1` plus unreleased changes)

## Purpose

Study Prime Agent as a reference implementation for an Orchest-based coding
agent. The work does not propose Prime Agent as a Multivac Runtime provider and
does not treat Prime Agent's product daemon as part of the Orchest SDK.

The research separates mature runtime lessons from Prime Agent's newer,
faster-moving mechanisms so that stable conclusions do not become coupled to
one release's implementation details.

## Document Set

1. `docs/research/sdk-comparison/orchest-vs-prime-agent-runtime-lessons.md`
   establishes the stable runtime comparison and the layer boundary used by
   the rest of the set.
2. `docs/research/sdk-comparison/prime-agent-rlm-and-retained-agents.md`
   studies asynchronous child admission, retained child lifecycles, run trees,
   and agent-to-agent messaging.
3. `docs/research/sdk-comparison/prime-agent-long-running-runtime.md` studies
   daemon continuity, input admission, scheduling, replay, and crash recovery.
4. `docs/research/sdk-comparison/prime-agent-persistent-execution-and-harness.md`
   studies persistent IPython, the typed host bridge, continual harness state,
   refinement, and rollback.
5. `docs/research/sdk-comparison/prime-agent-lessons-adoption-map.md` classifies
   findings as Adopt, Adapt, Reject, or Defer and assigns each accepted lesson
   to Orchest core, an Orchest Code Agent runtime layer, or a Code Agent service
   layer.

## Governing Boundary

The comparison uses three target layers:

- **Orchest core:** provider-neutral loop, state, events, tools, approvals,
  budgets, child-run control, and extension seams.
- **Orchest Code Agent runtime:** coding-specific execution sessions,
  workspace tools, context assembly, artifacts, and harness state.
- **Code Agent service:** worker residency, scheduling, reconnect, recovery,
  and process-level durability.

Prime Agent features enter Orchest core only when they are provider-neutral,
product-neutral, and necessary for more than a coding-agent product. This keeps
the research consistent with `docs/polaris/overview.md` and
`docs/polaris/non-goals.md`.

## Method

- Inspect Prime Agent's current source and architecture documentation rather
  than relying on the older pi-agent comparison.
- Distinguish claimed behavior from behavior evidenced by code or tests.
- Compare semantics, not language or package layout.
- Record what Orchest already does better so the result is not a feature
  shopping list.
- Prefer minimal contracts and extension seams over copying Prime Agent's
  product orchestration into the SDK.

## Acceptance

- The five documents have non-overlapping primary responsibilities and link to
  one another.
- Every recommendation names its target layer.
- Current v1.0 supervised-delegation gates are mapped rather than duplicated.
- Persistent execution preserves Orchest's Tool/MCP/Skill boundaries.
- Daemon, schedules, goals, and continual refinement are not silently promoted
  into Orchest core.
- The historical May 2026 pi-agent comparison is clearly marked as a historical
  baseline.

## Non-Goals

- Designing an integration adapter for Prime Agent.
- Selecting a concrete daemon transport or database.
- Adding implementation issues to the v1.0 release scope.
- Treating Prime Agent's default trust model as sufficient for an Orchest Code
  Agent.
- Replacing current authoritative iteration or Polaris contracts with research
  conclusions.
