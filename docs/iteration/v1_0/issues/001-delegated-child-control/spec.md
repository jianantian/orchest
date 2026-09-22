# 001 · Delegated child run control and completion

GitHub issue: #249

## Background

The v0.11 report records SB-1, SB-2, and P1-4: `AgentAsTool` retains its child
handle, public steering targets the supervisor, and child completion has no
public receiver.

## Goal

Expose one public delegated-child control surface that owns child-target
steering and completion without leaking private runtime paths.

## Acceptance Criteria

- [x] A caller can obtain or resolve a public control surface for the
  delegated child started by `AgentAsTool`.
- [x] Child-target inject and steer operations demonstrably change the child
  conversation and not the supervisor conversation.
- [x] Child completion/failure can be awaited without consuming the supervisor
  completion channel.
- [x] Existing supervisor-level steering behavior remains covered.
- [x] Public examples use no `pub(crate)` implementation paths.

## Notes

This is a v1.0 pre-freeze gate for SB-1 and SB-2. P1-4 shares the same
smallest repair surface.
