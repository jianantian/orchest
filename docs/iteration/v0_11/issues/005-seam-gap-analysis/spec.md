# 005 · Evidence validation, report, and release triage

## Background

Issues 001–004 update one canonical `findings.json`. The final report must be
a deterministic projection of that file, while live validation and v1.0
release gating remain explicit, separate decisions.

## Goal

Complete triage, validate the evidence graph, render the seam-gap report, and
record both iteration completion and the independent v1.0 readiness decision.

## Acceptance Criteria

- [ ] Every canonical finding has a stable identity, final classification,
  evidence refs, status, verification, and action ownership as required.
- [ ] The validator rejects broken refs and inconsistent readiness,
  classification, status, or verification states.
- [ ] `docs/iteration/v0_11/seam-gap-analysis.md` is generated only from
  `findings.json`.
- [ ] `render` is deterministic and `check` fails on stale Markdown.
- [ ] Fixture, test, smoke, and live-provider run records remain distinct.
- [ ] If credentials are available, the live run records exact command,
  revision, date, provider, model, outcome, and redacted diagnostic excerpt.
- [ ] If credentials are unavailable, the live run remains `not-run` and
  readiness is `unverified`; evidence collection may still complete.
- [ ] The report explicitly states whether `unverified` live evidence blocks
  v1.0 or is accepted by a named decision owner with rationale.
- [ ] Open seam/release blockers have bounded actions and issue references.
- [ ] v1.0 scope, Multivac M2 readiness, and the roadmap are updated from the
  reviewed canonical result.

## Report Contract

The rendered report contains the executive summary, readiness verdict, API
checklist, all findings, verification evidence, run evidence, live boundary,
and v1.0/Multivac implications. It introduces no independently edited facts.
