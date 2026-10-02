# 001 · SDK packaging ADR

GitHub issue: #324

## Background

Publishing the Python and Node packages needs decisions the later issues
depend on: package names, platforms, versioning and how publishing is
authenticated. The owner settled them on 2026-10-02 (see the SDK 0.2 PRD).
Nothing records them yet, and the `@orchest` npm scope has not been
confirmed.

## Goal

Record the decisions as ADR-0004 (`docs/adr/0004-sdk-packaging.md`) and
confirm the npm scope, so 002–007 have one source of truth.

## Acceptance Criteria

- [x] ADR-0004 states the package names (`orchest-py` importing as
  `orchest`; `orchest-sdk` plus three platform sub-packages) and the
  `orchest` import-name collision with orchest.io's package.
- [x] ADR-0004 lists the supported platforms (Linux x86_64 and arm64 on
  glibc with manylinux_2_28, macOS arm64) and the minimum Python (3.11) and
  Node (18) versions.
- [x] ADR-0004 states the versioning policy: one SDK version for both
  packages, independent of the crates, `sdk-vX.Y.Z` tags, and that 0.x
  makes no API stability promise. Each SDK release names the crate version
  it is built from.
- [x] ADR-0004 states the publishing policy: Trusted Publishing on PyPI
  and npm, a temporary npm token for the first publish only, and
  owner-approved publish environments.
- [x] The owner has confirmed the `@orchest` npm scope is available to
  them, or ADR-0004 records the fallback name `orchest-sdk` as chosen
  (the scope was unavailable; `orchest-sdk` is chosen).
- [x] ADR-0004 status is `Accepted` and names the decision owner.

## Blocked by

None — can start immediately.

## Notes

HITL: only the owner can create or confirm the npm organization.
