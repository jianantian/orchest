# v1.0 PRD: First Public Release

## Background

v1.0 is Orchest's first public crates.io release. The release freezes the
public Rust API only after the v0.10 and v0.11 product-validation evidence has
been reviewed and all pre-freeze/release gates have passed.

The canonical v0.11
[Seam Gap Analysis](../../review/v0_11_seam_gap_analysis.md) records eight open
supervised-delegation seam blockers, one open release blocker, and an
unexecuted required live-provider run. Deterministic v0.11 evidence may be
complete without making v1.0 ready.

## Goal

Publish the first stable Orchest release with:

- Cargo publish metadata;
- a finalized license;
- a release workflow;
- a changelog;
- a documented versioning strategy;
- reviewed supervised-delegation APIs;
- completed v0.10 and v0.11 live-provider validation.

## Pre-Freeze and Release Gates

These existing GitHub issues are v1.0 prerequisites:

| Issue | Gate | Evidence finding |
| --- | --- | --- |
| #249 | Delegated child run control and completion | SB-1, SB-2, P1-4 |
| #250 | Attached-watcher child-event delivery and formatting | SB-4, SB-8 |
| #251 | Delegated restart after run-level failure | SB-3 |
| #252 | Watcher event-loss recovery | SB-5 |
| #253 | Watcher attachment before execution | SB-6 |
| #254 | Deterministic multi-watcher action arbitration | SB-7 |
| #255 | Fallible `LlmWatcherBuilder` | RB-1 |
| #258 | v0.10/v0.11 live-provider verification | `run-live-provider` |

Issue #258 remains a release gate while the live run is `not-run` and
readiness is `unverified`. No waiver has been accepted.

## Post-1.0 Backlog

The reviewed report explicitly defers:

- #256: convenience re-exports for `LlmWatcher` and `ContextMode` (P1-1,
  P1-2);
- #257: reconciliation of the empty-parent Fork contract (P1-3).

These issues do not block v1.0.

## Out of Scope

- Reclassifying #256 or #257 as a release gate without new reviewed evidence.
- Treating v0.11 iteration completion as release readiness.
- Accepting unavailable live evidence without a named decision owner and
  recorded rationale.
- Adding product UI, user management, or multi-channel routing.

## Acceptance Criteria

- [ ] #249–#255 are closed with their declared verifier evidence.
- [ ] #258 records executed v0.10 and v0.11 live-provider evidence, and the
  v0.11 canonical readiness result is no longer `unverified` because of a
  missing required live run.
- [ ] The v0.11 generated report remains byte-current with its canonical JSON.
- [ ] Cargo publish metadata, license, release workflow, changelog, and
  versioning strategy are complete and reviewed.
- [ ] All workspace tests, lint checks, formatting checks, and documentation
  checks pass on the release candidate.

## Issue Documents

The current issue documents cover only the evidence-backed pre-freeze and
release gates:

1. [Delegated child control](issues/001-delegated-child-control/spec.md)
2. [Nested watcher events](issues/002-nested-watcher-events/spec.md)
3. [Run-level restart](issues/003-run-level-restart/spec.md)
4. [Watcher event-loss recovery](issues/004-watcher-event-loss/spec.md)
5. [Pre-run watcher attachment](issues/005-pre-run-watcher-attachment/spec.md)
6. [Watcher action arbitration](issues/006-watcher-action-arbitration/spec.md)
7. [Fallible LLM watcher builder](issues/007-fallible-llm-watcher-builder/spec.md)
8. [Live-provider verification](issues/008-live-provider-verification/spec.md)

Release-packaging work must receive real GitHub issue references before its
implementation documents are added.
