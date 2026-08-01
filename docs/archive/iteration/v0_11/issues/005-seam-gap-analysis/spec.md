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
- [ ] Every string that can reach the rendered report passes the same
  secret-shaped and machine-local privacy checks; environment variable names
  without values remain valid.
- [ ] Executed dates are calendar-valid `YYYY-MM-DD`, revisions are lowercase
  7–40 character Git object ids or the exact `git:self` post-commit verifier
  semantic, and source symbols reject case-insensitive `line`/`lines`/`L`
  locators and numeric single/range suffixes after `:` or `#`, including
  ranges joined by arbitrary non-digit separator text rather than a finite
  punctuation allowlist.
- [ ] `docs/iteration/v0_11/seam-gap-analysis.md` is generated only from
  `findings.json`.
- [ ] `render` and `check` resolve that one report against the verified
  repository root independently of the current working directory and reject
  symlink-parent escapes.
- [ ] `render` is deterministic and `check` fails byte-for-byte on stale
  Markdown without writing.
- [ ] Ordinary test targets never render over the tracked canonical report;
  path-resolution coverage uses an isolated temporary repository and
  wrong-working-directory `check` coverage proves bytes and metadata remain
  unchanged.
- [ ] Fixture, test, smoke, and live-provider run records remain distinct.
- [ ] `tests/smoke.rs` provides an explicitly ignored credential-gated
  provider path, and ordinary package tests do not call a live provider.
- [ ] If credentials are available, the live run records exact command,
  revision, date, provider, model, outcome, and redacted diagnostic excerpt.
- [ ] If credentials are unavailable, the live run remains `not-run` and
  readiness is `unverified`; evidence collection may still complete.
- [ ] Final passed worker, supervisor/watcher, failure-escalation, and
  watcher-order rows cite `git:self`, the containing commit where their
  claimed test targets exist and were rerun.
- [ ] The report explicitly states whether `unverified` live evidence blocks
  v1.0 or is accepted by a named decision owner with rationale.
- [ ] Open seam/release blockers have bounded actions and issue references.
- [ ] Aggregate seam- and release-blocker wording reports unresolved findings
  with their actual lifecycle status; `implemented` is never labelled `open`.
- [ ] v1.0 scope, Multivac M2 readiness, and the roadmap are updated from the
  reviewed canonical result.

## Report Contract

The rendered report contains the executive summary, readiness verdict, API
checklist, all findings, verification evidence, run evidence, live boundary,
and v1.0/Multivac implications. It introduces no independently edited facts.
