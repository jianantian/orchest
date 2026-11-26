# 007 · Release candidate and 0.2.0

GitHub issue: #330

## Background

Published PyPI and npm versions cannot be replaced, and npm's Trusted
Publishing can only be configured after a package exists. A release
candidate lets the packages be verified from the registries first, and
lets the owner switch npm to Trusted Publishing before the final release.

## Goal

Publish a release candidate on both PyPI and npm through Trusted Publishing,
verify it on all supported platforms, then publish `0.2.0` and close the
iteration.

The `sdk-v0.2.0-rc.1` workflow stopped before upload because `npm ci` rejected
the lockfile after npm's platform packages were created. npm already has the
four RC1 versions, while PyPI has no RC1. RC2 passed the SDK builds and
published its wheel set and sdist to PyPI. npm interpreted the relative
tarball paths as GitHub repository shorthand and stopped before uploading.
Use explicit local paths and cut RC3 for verification from both registries.

## Acceptance Criteria

- [x] The owner registered `orchest-py` as a pending trusted publisher on PyPI
  and configured Trusted Publishing for the four npm packages.
- [ ] The SDK packaging build can install its Node build dependencies before
  the target versions of the platform packages are available on npm.
- [ ] The SDK version is `0.2.0-rc.3`, `CHANGELOG-SDK.md` has its section,
  and the `sdk-v0.2.0-rc.3` tag publishes `orchest-py` `0.2.0rc3` to PyPI
  and the four npm packages `0.2.0-rc.3` under the `next` dist-tag.
- [ ] The post-publish verification passes on all three platforms for the
  release candidate.
- [ ] After owner sign-off, the `sdk-v0.2.0` tag publishes `0.2.0` to PyPI
  and npm (`latest`) through Trusted Publishing only, and the post-publish
  verification passes on all three platforms.
- [ ] Murmur installs the release candidate from PyPI instead of its git
  tag, and its existing calls run.
- [ ] The release notes give the SDK↔crate version matrix (`0.2.0` ↔
  crates `2.0.0`, ADR-0005 D6).
- [ ] The repository holds no long-lived PyPI or npm token.
- [ ] The PyPI and npm pages for 0.2.0 show provenance.
- [ ] The PRD's acceptance items are checked, the roadmap marks SDK 0.2
  completed, and `AGENTS.md` no longer says the binding packages are
  unpublished.
- [ ] The "being published" notes in `README.md` and both SDK guides
  are removed once `0.2.0` is on PyPI and npm.

## Blocked by

- #324, #325, #326, #327, #328, #329
- [v1.2](../../../v1_2/prd.md): the ASR streaming redesign (004)
- [v2.0](../../../v2_0/prd.md): the `2.0.0` crate release

## Notes

HITL: each publish approval and the 0.2.0 sign-off belong to the owner.
