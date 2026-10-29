# 003 · Node package publishable

GitHub issue: #326

## Background

`@orchest/sdk` ships one `orchest_node.node` built on the local machine,
copied by a hand-written script, and `js/index.js` loads that fixed path.
A published package would work on one platform only. On Linux the addon
links the system OpenSSL dynamically. The package is `UNLICENSED`.

## Goal

The main package and a platform sub-package can be packed and installed
together, and the loader picks the right native binary.

## Acceptance Criteria

- [x] `@napi-rs/cli` builds the addon as `orchest_node.<platform>.node` and
  generates the sub-packages `orchest-sdk-linux-x64-gnu`,
  `orchest-sdk-linux-arm64-gnu` and `orchest-sdk-darwin-arm64`.
- [x] The package is renamed from `@orchest/sdk` to `orchest-sdk`
  (ADR-0004 D2), and every `@orchest/sdk` reference in `js/`, the
  declarations, `js/tests`, `examples/typescript` and the binding tests
  uses the new name.
- [x] `orchest-sdk` lists the three sub-packages as `optionalDependencies`
  pinned to exactly its own version.
- [x] The loader uses a locally built addon when one is present, otherwise
  the sub-package for the current platform. `npm run build:native` and
  `npm test` keep working in the repository.
- [x] On an unsupported platform, including musl Linux, loading fails with
  an error that names the platform and lists the supported ones.
- [x] On Linux, the addon has no dynamic dependency on `libssl` /
  `libcrypto`, and an HTTPS request to a public endpoint verifies its
  certificate.
- [x] `package.json` and the sub-packages declare the license
  `MIT OR Apache-2.0`, the repository URL and `engines.node >= 18`.
- [x] Packing the main package and the current platform's sub-package,
  installing both into an empty directory and running the `js/tests` suite
  against the installed package passes.
- [x] `npm run typecheck` passes and `npm run build:types` leaves no diff.

## Blocked by

- #324 (SDK packaging ADR)

## Implementation Notes

- The loader lives in `js/load-native.js`; `js/tests/load-native.test.cjs`
  covers the unsupported-platform, musl, missing-sub-package and
  local-addon paths without a native build.
- `npm run build:native` writes `orchest_node.<platform>.node` to the
  repository root. `scripts/pack-node-sdk.sh OUT_DIR` copies each built
  addon into its `npm/<platform>/` package and packs it together with the
  main package; `npm run pack:local` does both for the current platform.
- The optional dependencies do not exist on npm before the release
  candidate; `npm install` and `npm ci` skip them. Once they are published,
  the version bump commit refreshes `package-lock.json`.
