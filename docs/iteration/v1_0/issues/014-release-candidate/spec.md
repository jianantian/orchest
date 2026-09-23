# 014 · Release candidate and 1.0.0 publish

GitHub issue: #311

## Background

The PRD's last acceptance item requires all workspace tests, lint checks,
formatting checks and documentation checks to pass on the release candidate.
Publishing 1.0.0 is irreversible, so a candidate is verified from crates.io
first.

## Goal

Cut and verify `1.0.0-rc.1`, then publish `1.0.0` and close v1.0.

## Acceptance Criteria

- [ ] The workspace version is `1.0.0-rc.1` and `CHANGELOG.md` is updated for
  it.
- [ ] The full `ci.yml` suite passes on the candidate commit, including
  `cargo doc` with warnings denied, `cargo build --examples` and
  `scripts/lint-check.sh`.
- [ ] Regenerating the public API inventory from 012 produces no diff.
- [ ] `1.0.0-rc.1` is published by the release workflow, and a scratch
  project depending on it from crates.io builds and runs a basic agent
  example.
- [ ] Before `1.0.0` is tagged, `docs/external/` and the `docs/analysis`
  symlink are removed or confirmed redistributable, and the repository is
  made public (ADR-0003 D8).
- [ ] After owner sign-off, `1.0.0` is published by the release workflow and
  its GitHub Release exists.
- [ ] The PRD's remaining acceptance items are checked and the roadmap marks
  v1.0 completed.

## Blocked by

- #306, #307, #308, #309, #310

## Notes

HITL: tagging `1.0.0` needs explicit owner sign-off.
