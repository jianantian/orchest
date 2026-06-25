# Issue 005: Omni evidence report

## Background

The purpose of v0.9.11 is not only to ship an experimental realtime provider path, but to gather evidence for the later provider-unification refactor. The iteration must close with a written report that separates observed provider facts from proposed abstractions.

## Goal / Scope

Run fake and credential-gated manual validation, then write the omni evidence report for the future provider-unification PRD/ADR.

In scope:

- Run required local checks.
- Run manual live validation when credentials are available.
- Document exact command, environment variables, provider, model, date and result.
- Record observed facts, provider-specific quirks and refactor recommendations separately.
- Update `docs/todo/provider-unification.md` only if Step 2 assumptions changed.

Out of scope:

- Do not start the provider-unification refactor in this issue.
- Do not create the final ADR unless the team explicitly chooses to start the refactor iteration.
- Do not mark unsupported provider capabilities as Orchest-wide limitations without evidence.

## Acceptance Criteria

- [x] `docs/iteration/v0_9_11/evidence.md` exists or an explicitly named ADR draft contains equivalent evidence.
- [x] The report includes exact validation commands and outcomes.
- [x] The report states whether audio input, audio output, text/transcript output, interruption and tool-use were observed.
- [x] The report lists refactor inputs for provider-core/unification, or explicitly states that insufficient evidence was collected.
- [x] `docs/todo/provider-unification.md` is updated only if the original Step 2 assumptions changed.
- [x] Required checks from the PRD are run or documented with environment limitations.

## Notes

This issue is the handoff point from evidence gathering to the later provider-unification PRD/ADR. Keep recommendations grounded in observed behavior.
