# 003 · Runtime tool flow

## Background

Briefing Desk must validate real Orchest runtime behavior: tools, metadata, approvals and event streaming. This issue turns the CLI skeleton into a functional local research agent.

## Goal

Implement the **text tools only**: search/read/write/report tools, approval behavior and event rendering. Multimedia tools (ASR transcription, vision image-in, TTS synthesis) are in issue 005 and must not be added here.

## Acceptance Criteria

- [ ] Search tool returns ranked local fixture paths and snippets.
- [ ] Read tool returns file contents with source path metadata.
- [ ] Report write tool marks side effects in `ToolMetadata`.
- [ ] Approval deny path leaves the output path absent or unchanged.
- [ ] Approval approve path writes exactly one Markdown report.
- [ ] CLI renders model, tool, approval and run-completion events to stdout.
- [ ] Tool errors are rendered with structured kind/code/next-step information when available.
- [ ] Fake-model smoke tests cover search, read, approval-deny and approval-approve paths.
- [ ] No private runtime APIs are used.

## Notes

This issue is the first place where `RetryHint`, structured errors or approval ergonomics may become release-blocker findings. Record friction in the validation report rather than expanding scope inline.
