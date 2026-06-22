# 002 · Handoff transition safety

## Background

Handoff processing currently mutates several pieces of run state in sequence. If filtering or state rebuilding fails midway, the run can become incoherent.

## Goal

Make handoff transition state construction atomic from the perspective of `AgentRunState`, including failures from handoff input filters.

## Acceptance Criteria

- [x] Handoff next messages are computed before replacing `state.messages`.
- [x] `HandoffInputFilter::filter` returns a fallible result such as `Result<HandoffInputData, HandoffError>` or the local equivalent.
- [x] Next registry and tool definitions are built before replacing the active registry/tool definitions.
- [x] Budget/config decisions are computed before active state replacement.
- [x] If handoff filtering fails, previous state remains coherent and the run emits a structured failure.
- [x] Tests cover successful handoff and handoff filter failure.
- [x] Public examples that implement handoff filters are updated for the fallible filter signature.
- [x] Runtime event ordering for successful handoff is documented or preserved.

## Notes

Snapshot-then-swap is preferred for v0.9.5. A larger "close current run and restart under supervisor" design can remain future work unless implementation proves snapshot-then-swap insufficient.
