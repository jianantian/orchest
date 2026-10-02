# 005 · SDK release workflow

GitHub issue: #328

## Background

`release.yml` publishes only the Rust crates. Nothing publishes the SDK
packages, checks that their versions agree, or records SDK changes. A
version on PyPI or npm cannot be replaced once published.

## Goal

An `sdk-vX.Y.Z` tag publishes both SDK packages through one workflow that
verifies first, waits for owner approval, publishes with provenance and
then verifies the published packages.

## Acceptance Criteria

- [ ] A version check script fails unless `package.json`,
  `crates/orchest-py/Cargo.toml` and `crates/orchest-node/Cargo.toml` carry
  the same version, the sub-package pins match it, the tag (when given)
  matches it, and `CHANGELOG-SDK.md` has a section for it.
- [ ] `CHANGELOG-SDK.md` exists in Keep a Changelog format with an
  `Unreleased` section, and `WORKFLOW.md` says when to add entries.
- [ ] `release-sdk.yml` runs on `sdk-vX.Y.Z` and `sdk-vX.Y.Z-rc.N` tags and
  on `workflow_dispatch` with a `dry_run` input. A real publish must run
  from a tag.
- [ ] It runs the version check, then the SDK job from 004, before
  anything is published.
- [ ] The PyPI and npm publish jobs run in GitHub environments that require
  owner approval.
- [ ] PyPI: the wheels and the sdist are uploaded through Trusted
  Publishing. A pre-release tag publishes the matching PEP 440
  pre-release version.
- [ ] npm: the three sub-packages are published before the main package,
  with provenance. The workflow uses the `NPM_TOKEN` secret when it is
  set and Trusted Publishing otherwise. A pre-release publishes under the
  `next` dist-tag, a release under `latest`.
- [ ] A rerun skips versions that are already on PyPI or npm.
- [ ] It creates a GitHub Release for the tag from the
  `CHANGELOG-SDK.md` section, marked prerelease for pre-release versions.
- [ ] After publishing, a verification job installs the published version
  from PyPI and npm on each of the three platforms and runs the SDK tests.
- [ ] A `dry_run` run stops after the build and tests, uploads nothing,
  succeeds, and is linked from this issue.

## Blocked by

- #327 (SDK CI)

## Notes

GitHub only dispatches workflows that exist on the default branch, so the
`dry_run` criterion is verified after this issue's commit is on `main`.
