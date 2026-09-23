# 009 · Release policy decision

GitHub issue: #306

## Background

v1.0 is the first public crates.io release. Publishing needs decisions that
are the project owner's to make and that every later packaging issue
depends on: the license, which workspace members are published, and how
versions evolve after 1.0. None of these are recorded today — no crate
declares a license and all members sit at `0.1.0`.

## Goal

Record the release policy as an accepted ADR (`docs/adr/0003-release-policy.md`)
so metadata, changelog, API review and the release workflow have one source
of truth.

## Acceptance Criteria

- [x] The ADR states the license as an SPDX expression with its rationale.
- [x] The ADR classifies every workspace member as published or
  `publish = false`; `orchest-py`, `orchest-node` and the demo examples are
  `publish = false` (binding packages are out of v1.0 scope).
- [x] The ADR states the versioning scheme (lockstep or per-crate), what counts
  as a breaking change (public API definition, feature flags, public
  dependency upgrades) and the MSRV value and bump policy.
- [x] The ADR states the release tag format and the crate publish order.
- [x] The ADR status is `Accepted` and names the decision owner.

## Blocked by

None — can start immediately.

## Notes

HITL: the decisions belong to the project owner; the agent drafts options
and the owner chooses.
