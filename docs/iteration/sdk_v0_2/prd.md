# SDK 0.2 PRD: Publish the Python and Node Packages

## Background

The eight Rust crates are on crates.io (1.1.0), but the Python and
TypeScript SDKs can only be built from source: `maturin develop` and
`npm run build:native` both need a local Rust toolchain. The v1.0 PRD left
binding packages out of scope. The README's Python and TypeScript examples
therefore cannot be installed by a user today.

State on 2026-10-02:

- **Python:** `pyproject.toml` builds with maturin; distribution
  `orchest-py` 0.1.0, import name `orchest`, `requires-python >= 3.11`.
  The extension is not built against the stable ABI.
- **Node:** `package.json` is `@orchest/sdk` 0.1.0, `UNLICENSED`, and ships
  one locally built `orchest_node.node`, so a published package would work
  on one platform only.
- **Linux:** both bindings link the system OpenSSL dynamically
  (`reqwest` → `native-tls` → `openssl-sys`).
- **CI and release:** CI builds and tests the bindings on Linux x86_64
  only. `release.yml` publishes only the Rust crates.
- **Names:** `orchest` is taken on PyPI and npm by an unrelated project
  (orchest.io), whose Python package also imports as `orchest`.
  `orchest-py` is free on PyPI. `@orchest/sdk` is unpublished and the
  `@orchest` scope shows no packages; its ownership is unconfirmed.

## Goal

A user on Linux x86_64, Linux arm64 or macOS arm64 runs
`pip install orchest-py` or `npm install @orchest/sdk` and gets a working
SDK without a Rust toolchain. Packages are built in CI and published
through Trusted Publishing with provenance.

## Decisions

These were settled with the owner on 2026-10-02 and are recorded in
ADR-0004 (issue 001).

| Topic | Decision |
| --- | --- |
| Python name | Distribution `orchest-py`, import `orchest`. It cannot be installed next to orchest.io's `orchest` package; the README says so. |
| npm name | `@orchest/sdk`, provided the owner holds the `@orchest` scope. Fallback: unscoped `orchest-sdk`. |
| Platforms | Linux x86_64 (glibc), Linux arm64 (glibc), macOS arm64. Linux wheels target manylinux_2_28. |
| Versioning | One SDK version shared by both packages, independent of the crates, starting at `0.2.0`. Tags are `sdk-vX.Y.Z` / `sdk-vX.Y.Z-rc.N`. 0.x makes no API stability promise. |
| Authentication | Trusted Publishing (OIDC) on PyPI and npm. npm's first publish uses a temporary token, because npm needs the package to exist before a trusted publisher can be configured. |
| Node packaging | Main package plus one sub-package per platform, listed as `optionalDependencies` pinned to the same version. |
| Python packaging | One stable-ABI (`cp311-abi3`) wheel per platform, plus an sdist. Fallback if abi3 is not feasible: one wheel per Python version, 3.11 to 3.14. |
| Linux TLS | The binding crates compile OpenSSL in statically (vendored). The published crates are unchanged. |
| Changelog | SDK releases are recorded in `CHANGELOG-SDK.md`. |
| Publish gate | Publishing jobs run in GitHub environments that require owner approval. |

## Scope

- Python package: stable ABI, static OpenSSL on Linux, version read from
  `crates/orchest-py/Cargo.toml`, license / README / URL metadata.
- Node package: `@napi-rs/cli`, three platform sub-packages, a loader that
  picks the platform package and reports unsupported platforms clearly,
  static OpenSSL on Linux, license `MIT OR Apache-2.0`.
- CI: an SDK job that release-builds on the three platforms, installs the
  built packages and runs the existing SDK tests. Linux x86_64 also tests
  the minimum and the latest Python and Node versions. The job runs on
  pushes to `main`, on PRs that touch binding paths, and from the release
  workflow.
- Release: `release-sdk.yml`, an SDK version check script and
  `CHANGELOG-SDK.md`.
- Documentation: install instructions, supported platforms, minimum
  versions and the `orchest` import-name collision.
- Release candidate `0.2.0-rc.1`, then `0.2.0`.

## Out of Scope

- Windows, Intel macOS and musl (Alpine) builds.
- An API freeze review or a 1.0 for the SDKs.
- New SDK features.
- A new Rust crate release. `orchest-py` and `orchest-node` stay
  `publish = false` on crates.io.
- Switching the published crates from `native-tls` to rustls.

## Issues

In dependency order:

| Doc | Issue | Work | Type | Blocked by |
| --- | --- | --- | --- | --- |
| 001 | #324 | ADR-0004 and npm scope confirmation | HITL | — |
| 002 | #325 | Python package publishable | AFK | #324 |
| 003 | #326 | Node package publishable | AFK | #324 |
| 004 | #327 | Three-platform SDK CI | AFK | #325, #326 |
| 005 | #328 | SDK release workflow | AFK | #327 |
| 006 | #329 | Install documentation | AFK | #325, #326 |
| 007 | #330 | Release candidate and 0.2.0 | HITL | #324–#329 |

002 and 003 are independent of each other. 006 can run alongside 004 and
005. The main risks are in 002 and 003: whether the stable ABI is
feasible, and whether static OpenSSL builds and finds CA certificates on
both Linux architectures.

1. [SDK packaging ADR](issues/001-sdk-packaging-adr/spec.md)
2. [Python package](issues/002-python-package/spec.md)
3. [Node package](issues/003-node-package/spec.md)
4. [SDK CI](issues/004-sdk-ci/spec.md)
5. [SDK release workflow](issues/005-sdk-release-workflow/spec.md)
6. [Install documentation](issues/006-sdk-install-docs/spec.md)
7. [Release candidate and 0.2.0](issues/007-sdk-release/spec.md)

## Acceptance Criteria

- [ ] On a clean runner for each of the three platforms, installing
  `orchest-py` 0.2.0 from PyPI and `@orchest/sdk` 0.2.0 from npm, with no
  Rust toolchain, passes the existing SDK tests.
- [ ] The repository holds no long-lived PyPI or npm token after the
  release.
- [ ] The PyPI and npm pages for 0.2.0 show provenance for the published
  files.
- [ ] ADR-0004 is `Accepted`, and the README and SDK guides describe
  installation from PyPI and npm.

## Owner Setup

| # | Step | When |
| --- | --- | --- |
| 1 | Create or confirm the `orchest` organization on npm | Before 003 |
| 2 | Register `orchest-py` on PyPI as a pending trusted publisher for `release-sdk.yml` | Before the RC |
| 3 | Add a temporary npm token to the repository secrets | Before the RC |
| 4 | Configure trusted publishers for the four npm packages, then delete the token | After the RC, before 0.2.0 |
