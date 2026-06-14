# 004 · Session resume and reviewer path

## Background

A complete SDK validation demo needs more than a one-shot run. It should prove that session persistence and one sub-agent or handoff path are usable from an application.

## Goal

Add persisted session resume and a reviewer sub-agent or handoff that checks the draft report before final write approval.

## Acceptance Criteria

- [ ] Initial run persists session state to the configured session path.
- [ ] `resume` loads the saved session and appends a follow-up answer.
- [ ] Resume flow preserves enough context for the follow-up answer to reference the original brief.
- [ ] Reviewer path uses either Agent-as-Tool or Handoff through public APIs.
- [ ] Reviewer output is visible in the event stream or final validation summary.
- [ ] Fake-model tests cover session resume.
- [ ] Validation notes classify any sub-agent context friction as demo blocker, release blocker or post-1.0 backlog.

## Notes

Do not add new sub-agent API shape in this issue unless the existing public API blocks the demo. If that happens, document the blocker first.
