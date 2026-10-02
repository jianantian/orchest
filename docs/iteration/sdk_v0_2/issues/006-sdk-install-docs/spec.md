# 006 · Install documentation

GitHub issue: #329

## Background

The README and the SDK guides tell Python and TypeScript users to build
from source. Once the packages are published, those instructions are
wrong for most users, and nothing explains which platforms are supported
or that `orchest-py` shares its import name with another project.

## Goal

A new user can install either SDK from the documentation alone and knows
the limits before hitting them.

## Acceptance Criteria

- [ ] `README.md`, `docs/guide/sdk-python.md` and
  `docs/guide/sdk-typescript.md` lead with `pip install orchest-py` and
  `npm install orchest-sdk`, and keep the
  build-from-source steps as a contributor path.
- [ ] They list the supported platforms and the minimum Python and Node
  versions from ADR-0004, and say what happens on other platforms: Python
  builds the sdist with a Rust toolchain, Node fails with the loader's
  error.
- [ ] The Python guide and the Python package README state that
  `orchest-py` cannot be installed alongside orchest.io's `orchest`
  package, because both import as `orchest`.
- [ ] The docs state that the SDKs are 0.x, versioned independently of the
  crates, and point to `CHANGELOG-SDK.md`.
- [ ] Every install command and package name in these files matches the
  published package names.

## Blocked by

- #325 (Python package)
- #326 (Node package)
