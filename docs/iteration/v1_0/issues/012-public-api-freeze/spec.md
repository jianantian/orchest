# 012 · Public API freeze review

GitHub issue: #309

## Background

v1.0 freezes the public Rust API; after it, any breaking change needs a major
version. The pre-freeze gates fixed known seam problems (#249–#255), but the
full public surface of the published crates has not been reviewed as a whole.

## Goal

Review and approve the public API of every published crate before 1.0.0, and
leave an inventory that later changes can be diffed against.

## Acceptance Criteria

- [ ] A public API inventory for each published crate is committed under
  `docs/review/`, produced by a documented, reproducible command.
- [ ] Every public enum and struct expected to grow is `#[non_exhaustive]`,
  or the review records it as intentionally exhaustive.
- [ ] Items not intended as public API are made non-public or
  `#[doc(hidden)]`, with the reason recorded in the review.
- [ ] Third-party types exposed in public signatures are listed in the review
  as public dependencies under the ADR's SemVer policy.
- [ ] The review records the owner's approval per crate.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and
  the CI `cargo doc` step pass after the changes, and bindings and examples
  still build.

## Blocked by

- #306 (release policy)

## Notes

HITL: the agent prepares the inventory and proposed changes; the owner
approves.
