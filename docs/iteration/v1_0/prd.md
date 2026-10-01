# v1.0 PRD: First Public Release

## Background

v1.0 is Orchest's first public crates.io release. The release freezes the
public Rust API only after the v0.10 and v0.11 product-validation evidence has
been reviewed and all pre-freeze/release gates have passed.

The canonical v0.11
[Seam Gap Analysis](../../review/v0_11_seam_gap_analysis.md) recorded eight
open supervised-delegation seam blockers, one open release blocker, and an
unexecuted required live-provider run when this PRD was written. As of the
2026-09-22 evidence update: SB-1–SB-8 and RB-1 are `verified`, both required
live-provider runs are `passed` (normal run and controlled-fault drill), and
the canonical readiness verdict is `ready`; the backlog the round produced is
`verified` (P1-6 → #298, plus #299 and #300, fixed in #302, #303 and #305).
The ledger's live evidence awaits review; the remaining v1.0 acceptance items
are the release-packaging ones below, tracked as #306–#311.

**Status (2026-10-01):** released. All eight crates are published to
crates.io at `1.0.0`, the
[GitHub Release](https://github.com/jianantian/orchest/releases/tag/v1.0.0)
exists, and the repository is public. Every acceptance item below is met.

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
| #258 | v0.10/v0.11 live-provider verification | `run-live-provider`, `run-live-provider-controlled-fault` |

Issue #258 was a release gate while the live run was `not-run` and readiness
was `unverified`. Both required live runs are now recorded in the canonical
ledger as `passed`, readiness is `ready`, and the gate is closed on that
evidence. No waiver has been accepted.

## Release-Packaging Issues

The remaining acceptance items are split into these issues, in dependency
order:

| Issue | Work | Blocked by |
| --- | --- | --- |
| #306 | Release policy ADR: license, publish set, versioning, MSRV | — |
| #307 | Cargo publish metadata and license files | #306 |
| #308 | `CHANGELOG.md` with the 1.0.0 entry | #306 |
| #309 | Public API freeze review | #306 |
| #310 | Tag-triggered release workflow | #307, #308 |
| #311 | Release candidate verification and 1.0.0 publish | #306–#310 |

Python and Node binding packages (PyPI, npm) are not part of v1.0; the
binding crates are `publish = false`.

## Post-1.0 Backlog

The reviewed report deferred, and these are now closed:

- #256: convenience re-exports for `LlmWatcher` and `ContextMode` (P1-1,
  P1-2) — closed by #301;
- #257: reconciliation of the empty-parent Fork contract (P1-3) — closed by
  #304.

The #258 live-validation round added three more, all closed as well:

- #298: default `LlmWatcher` abort authority (P1-6) — closed by #302;
- #299: `input_mapper` without a matching `input_schema` — closed by #303;
- #300: `SynthesizeRequest.voice` semantics for `None` — closed by #305.

None of these block v1.0.

## Out of Scope

- Reclassifying #256 or #257 as a release gate without new reviewed evidence.
- Treating v0.11 iteration completion as release readiness.
- Accepting unavailable live evidence without a named decision owner and
  recorded rationale.
- Adding product UI, user management, or multi-channel routing.
- Publishing the Python or Node binding packages.

## Acceptance Criteria

- [x] #249–#255 are closed with their declared verifier evidence.
- [x] #258 records executed v0.10 and v0.11 live-provider evidence, and the
  v0.11 canonical readiness result is no longer `unverified` because of a
  missing required live run.
- [x] The v0.11 generated report remains byte-current with its canonical JSON.
- [x] Cargo publish metadata, license, release workflow, changelog, and
  versioning strategy are complete and reviewed.
- [x] All workspace tests, lint checks, formatting checks, and documentation
  checks pass on the release candidate.

## Issue Documents

Pre-freeze and release gates:

1. [Delegated child control](issues/001-delegated-child-control/spec.md)
2. [Nested watcher events](issues/002-nested-watcher-events/spec.md)
3. [Run-level restart](issues/003-run-level-restart/spec.md)
4. [Watcher event-loss recovery](issues/004-watcher-event-loss/spec.md)
5. [Pre-run watcher attachment](issues/005-pre-run-watcher-attachment/spec.md)
6. [Watcher action arbitration](issues/006-watcher-action-arbitration/spec.md)
7. [Fallible LLM watcher builder](issues/007-fallible-llm-watcher-builder/spec.md)
8. [Live-provider verification](issues/008-live-provider-verification/spec.md)

Release packaging:

9. [Release policy](issues/009-release-policy/spec.md)
10. [Publish metadata and license](issues/010-publish-metadata/spec.md)
11. [Changelog](issues/011-changelog/spec.md)
12. [Public API freeze review](issues/012-public-api-freeze/spec.md)
13. [Release workflow](issues/013-release-workflow/spec.md)
14. [Release candidate and 1.0.0 publish](issues/014-release-candidate/spec.md)
