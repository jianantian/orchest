# 007 · Release candidate and 0.2.0

GitHub issue: #330

## Background

Published PyPI and npm versions cannot be replaced, and npm's Trusted
Publishing can only be configured after a package exists. A release
candidate lets the packages be verified from the registries first, and
lets the owner switch npm to Trusted Publishing before the final release.

## Goal

Publish `0.2.0-rc.1`, verify it from PyPI and npm, move npm to Trusted
Publishing, then publish `0.2.0` and close the iteration.

## Acceptance Criteria

- [ ] The owner has registered `orchest-py` as a pending trusted publisher
  on PyPI and added a temporary `NPM_TOKEN` secret.
- [ ] The SDK version is `0.2.0-rc.1`, `CHANGELOG-SDK.md` has its section,
  and the `sdk-v0.2.0-rc.1` tag publishes `orchest-py` `0.2.0rc1` to PyPI
  and the four npm packages `0.2.0-rc.1` under the `next` dist-tag.
- [ ] The post-publish verification passes on all three platforms for the
  release candidate.
- [ ] The owner has configured trusted publishers for the four npm
  packages and deleted the `NPM_TOKEN` secret.
- [ ] After owner sign-off, the `sdk-v0.2.0` tag publishes `0.2.0` to PyPI
  and npm (`latest`) through Trusted Publishing only, and the post-publish
  verification passes on all three platforms.
- [ ] The repository holds no long-lived PyPI or npm token.
- [ ] The PyPI and npm pages for 0.2.0 show provenance.
- [ ] The PRD's acceptance items are checked, the roadmap marks SDK 0.2
  completed, and `AGENTS.md` no longer says the binding packages are
  unpublished.

## Blocked by

- #324, #325, #326, #327, #328, #329

## Notes

HITL: the PyPI and npm setup, each publish approval and the 0.2.0 sign-off
belong to the owner.
