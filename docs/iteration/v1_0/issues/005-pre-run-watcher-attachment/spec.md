# 005 · Watcher attachment before run execution

GitHub issue: #253

## Background

SB-6 proves `AgentRun::start` schedules execution before callers can attach a
watcher. Application code cannot guarantee observation from the first event.

## Goal

Add a public start-with-watchers or pause-and-activate seam that makes
first-event watcher observation deterministic.

## Acceptance Criteria

- [ ] A caller can register declared watchers before the first model call is
  released.
- [ ] Successful registration guarantees observation from the documented
  first runtime boundary.
- [ ] Registration failure is returned before execution begins.
- [ ] Existing post-start `attach_watcher` behavior remains compatible.
- [ ] A non-fixture integration test proves first-event observation without
  application timing assumptions.

## Notes

This is a v1.0 pre-freeze gate.
