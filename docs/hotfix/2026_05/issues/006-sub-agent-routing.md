# 006 · Repair Sub-Agent Routing, Events, and Budget

## Background

Sub-agent support currently starts child runs and forwards events, but several documented guarantees are missing:

- approval responses cannot be routed to child runs
- parent budget is updated only after child completion
- events do not carry true `run_depth` / `child_run_id` metadata
- sub-agent tool/skill permissions can be narrowed only partially and expansion is not explicitly rejected

## Goal

Make sub-agent behavior safe, observable, and non-blocking in approval and budget scenarios.

## Acceptance Criteria

**Approval routing:**
- [x] `RunHandle::respond_approval(run_id, approved)` routes to the matching active run, including sub-agents
- [x] If `run_id` is unknown or no approval is pending, the method returns a clear error or emits a warning instead of silently sending to root
- [x] A test covers a sub-agent tool requiring approval and verifies approval grants execution
- [x] A test covers sub-agent approval denial and verifies child run completes or fails according to documented semantics without deadlock

**Event identity:**
- [x] Runtime events contain enough identity metadata to distinguish root and child run events without SDK-side fake defaults
- [x] Sub-agent events preserve the child run id
- [x] Events include run depth or an equivalent explicit nesting marker
- [x] Python and TypeScript event types expose this metadata

**Budget propagation:**
- [x] Parent budget usage updates as child `ModelCallCompleted` and `ToolCallCompleted` events arrive
- [x] Parent budget guard can stop additional parent work after child usage exhausts the parent budget
- [x] Child budget remains capped by parent remaining budget

**Permission inheritance:**
- [x] Sub-agent `allowed_tools` and `allowed_skills` inherit parent restrictions by default
- [x] Child config can narrow but not expand parent restrictions
- [x] Tests cover attempted expansion of both tools and skills
- [x] The filtering behavior reuses the permission semantics defined in hotfix issue 001

## Notes

SDK injection criteria (`orchest_sdk` / `orchest-sdk` availability for skill scripts) have been moved to issue 003 where they belong with skill loading infrastructure.

Avoid representing child metadata only in lifecycle wrapper events. Consumers need to correlate ordinary child `ModelStreamChunk`, tool, approval, and completion events too.
