# SDK Changelog

All notable changes to the Python package `orchest-py` and the npm package
`orchest-sdk` are documented in this file. The Rust crates have their own
[CHANGELOG.md](CHANGELOG.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The SDKs share one version, independent of the crate version
([ADR-0004](docs/adr/0004-sdk-packaging.md) D6). While the version is 0.x,
minor releases may break the Python or TypeScript API; such changes are
marked **Breaking:**. Each release names the crate version it is built
from.

## [Unreleased]

### Added

- First published release of `orchest-py` (PyPI) and `orchest-sdk` (npm),
  with prebuilt packages for Linux x86_64, Linux arm64 and macOS arm64
  ([ADR-0004](docs/adr/0004-sdk-packaging.md)).

### Changed

- **Breaking:** the npm package is `orchest-sdk`; it was `@orchest/sdk`
  when built from source.
