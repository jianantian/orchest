# 004 · Watcher event-loss recovery

GitHub issue: #252

## Background

SB-5 records that secondary watcher subscribers use `try_send`, may lose
events under backpressure, and have no recovery path.

## Goal

Provide an explicit observable recovery or replay contract for watcher event
loss without silently weakening runtime progress.

## Acceptance Criteria

- [x] A saturated watcher cannot lose events without an attributable signal.
- [x] The public contract defines how a watcher recovers, replays, or resumes
  after the loss signal.
- [x] A deterministic saturation test proves the declared recovery boundary.
- [x] Normal no-drop FIFO behavior remains unchanged.
- [x] Recovery state is bounded and cannot grow without limit.

## Notes

This is a v1.0 pre-freeze gate; the implementation choice must preserve the
runtime's minimal-core boundary.
