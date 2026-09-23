# 013 · Release workflow

GitHub issue: #310

## Background

CI (`ci.yml`) only verifies pushes and pull requests. Nothing publishes
crates or creates GitHub releases, so a release today would be a manual,
unrepeatable sequence of `cargo publish` calls.

## Goal

Publish a release from a version tag through one workflow that verifies
first, publishes in dependency order and creates the GitHub Release.

## Acceptance Criteria

- [x] A release workflow runs on tags in the ADR format and on
  `workflow_dispatch` with a `dry_run` input.
- [x] It fails before publishing when the tag version differs from the
  workspace version or `CHANGELOG.md` has no section for that version.
- [x] It runs the same checks as `ci.yml` before publishing.
- [x] It publishes the published crates in the ADR order using the
  `CARGO_REGISTRY_TOKEN` secret, and a rerun skips versions already on
  crates.io.
- [x] It creates a GitHub Release whose body is the changelog section, marked
  prerelease when the version has a pre-release suffix.
- [ ] A `dry_run` run succeeds and is linked from this issue.
- [x] `WORKFLOW.md` documents the release steps and the required secret.

## Blocked by

- #307 (publish metadata)
- #308 (changelog)

## Notes

GitHub only dispatches workflows that exist on the default branch, so the
`dry_run` criterion is verified after `iteration/v1_0` merges to `main`,
before #311 cuts the release candidate.
