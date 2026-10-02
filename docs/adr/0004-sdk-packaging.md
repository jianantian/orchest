# ADR-0004: Python and Node SDK Packaging

**Status:** Accepted
**Date:** 2026-10-02 (accepted 2026-10-02)
**Deciders:** Orchest maintainer (emile) — decision owner
**Related:** [SDK 0.2 PRD](../iteration/sdk_v0_2/prd.md),
[#324](https://github.com/jianantian/orchest/issues/324),
[ADR-0003](./0003-release-policy.md)

## Context

The Rust crates are published on crates.io under ADR-0003. Their version is
1.1.0 at the time of writing. The Python (`orchest-py`, PyO3 + maturin) and
TypeScript (`orchest-node`, napi-rs) bindings have never been published.
Both binding crates are `publish = false`, ADR-0003 D2 put binding packages
out of v1.0 scope, and users have to build them from source with a Rust
toolchain.

Facts that constrain the decisions (2026-10-02):

- `orchest` is taken on PyPI (orchest.io's SDK, latest 0.3.11) and on npm.
  orchest.io's Python package also imports as `orchest`.
- `orchest-py` is free on PyPI. The `orchest` npm organization name is
  unavailable (confirmed by the owner, 2026-10-02), so the `@orchest` scope
  cannot be used. `orchest-sdk` and the matching platform package names are
  free on npm.
- On Linux both bindings reach the system OpenSSL dynamically:
  `reqwest` → `native-tls` → `openssl-sys`. A prebuilt binary linked
  against one OpenSSL fails to load on a host with another.
- A release build of the Node addon is about 14 MB per platform.
- Nothing published to PyPI or npm can be replaced under the same version.

## Decisions

### D1. Python distribution and import name

The distribution is **`orchest-py`** and the import name stays **`orchest`**.

The import name collides with orchest.io's package, so the two cannot be
installed in the same environment: they would overwrite each other's
files. That project appears dormant, and renaming the import would break
every existing Python user and drift from the crate name. The README, the
Python guide and the package README state the collision.

### D2. npm package name

The npm package is the unscoped **`orchest-sdk`**, with platform packages
named to match (D4).

The first choice was `@orchest/sdk`, the name the repository already uses.
It needs the `orchest` npm organization, which the owner found
unavailable on 2026-10-02 (#324). Every `@orchest/sdk` reference in code,
declarations, tests, examples and guides moves to `orchest-sdk` in #326
and #329. Unscoped names can be claimed by anyone before the first
publish; the release candidate (#330) claims all four.

### D3. Supported platforms and runtimes

| Platform | Target |
| --- | --- |
| Linux x86_64 | glibc, `manylinux_2_28` |
| Linux arm64 | glibc, `manylinux_2_28` |
| macOS arm64 | Apple silicon |

The minimum runtimes are Python 3.11 and Node 18. Windows, Intel macOS and
musl Linux (Alpine) are not built. Python users elsewhere can build from
the sdist with a Rust toolchain. Node loading on another platform fails
with an error that names the platform and lists the supported ones.
Adding a platform later is an additive change.

### D4. Package layout

- **Python:** one stable-ABI wheel per platform (`cp311-abi3`, PyO3
  `abi3-py311`) covering every Python from 3.11, plus one sdist. If the
  stable ABI proves infeasible (#325), the fallback is one wheel per Python
  version from 3.11 to 3.14. Whichever is used is recorded here.
- **Node:** the main package `orchest-sdk` holds only JavaScript and type
  declarations. It lists one sub-package per platform as
  `optionalDependencies`, pinned to exactly its own version:
  `orchest-sdk-linux-x64-gnu`, `orchest-sdk-linux-arm64-gnu` and
  `orchest-sdk-darwin-arm64`. This is napi-rs's standard layout, so npm
  downloads only the current platform's ~14 MB binary, and new platforms
  add sub-packages without growing anyone's download.

### D5. TLS on Linux

The binding crates compile OpenSSL in statically (vendored) on Linux, so
the wheels and the addon do not depend on the host's `libssl` /
`libcrypto`. Certificate discovery must still find the system CA store.
The published Rust crates are unchanged: their dependencies and features
stay as ADR-0003 froze them. Moving the crates to rustls is a separate
decision.

### D6. Versioning

- `orchest-py` and `orchest-sdk`, with its platform sub-packages, share
  **one SDK version**, independent of the crate version. It starts at
  `0.2.0`.
- **0.x makes no API stability promise.** Breaking changes to the Python
  or TypeScript API are allowed in minor releases and are marked
  **Breaking:** in the changelog. A 1.0 for the SDKs needs its own API
  freeze review, in the style of #309.
- The version is written in three places: `package.json`,
  `crates/orchest-py/Cargo.toml` and `crates/orchest-node/Cargo.toml`.
  `pyproject.toml` reads it from the crate manifest, and maturin converts
  Cargo pre-releases (`0.2.0-rc.1`) to PEP 440 (`0.2.0rc1`). A check script
  keeps the three in agreement and matching the tag.
- Tags are `sdk-vX.Y.Z` and `sdk-vX.Y.Z-rc.N`. SDK releases are recorded
  in `CHANGELOG-SDK.md`, and each release names the crate version it is
  built from.
- An SDK release does not require a crate release, and a crate release
  does not force an SDK release.

### D7. Publishing

- **Trusted Publishing (OIDC)** on both PyPI and npm, so no long-lived
  token is stored and the published files carry provenance.
- npm only lets a trusted publisher be configured once a package exists.
  The first publish (the release candidate) therefore uses a temporary
  `NPM_TOKEN` secret. The owner then configures trusted publishers for all
  four packages and deletes the secret before 0.2.0.
- Publishing runs in GitHub environments that require **owner approval**,
  after the version check and the full SDK build and test have passed.
- After publishing, the workflow installs the published version from PyPI
  and npm on each supported platform and runs the SDK tests.
- npm pre-releases go to the `next` dist-tag and releases to `latest`.
  A failed release is fixed with a new version, never by replacing one.

## Consequences

- #325 implements D1, D3–D6 for Python: stable ABI, vendored OpenSSL,
  dynamic version, metadata.
- #326 implements D2–D6 for Node: platform packages, loader, vendored
  OpenSSL, license `MIT OR Apache-2.0`.
- #327 builds and tests the packages on the D3 platforms.
- #328 implements D6's version check and changelog, and D7's workflow.
- #329 documents D1's collision, D3's platforms and D6's versioning.
- #330 carries out D7's release-candidate-then-trusted-publishing
  sequence.
