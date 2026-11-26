# 003 implementation plan

## Files to read

- `docs/adr/0004-sdk-packaging.md`
- `package.json`, `js/index.js`, `js/native.d.ts`, `js/tests/`
- `crates/orchest-node/Cargo.toml`, `scripts/copy-node-addon.mjs`

## Files to change

- `package.json` (napi configuration, scripts, license,
  `optionalDependencies`)
- `npm/<platform>/package.json` for the three sub-packages (new)
- `js/index.js` (loader) and its generated declarations
- `crates/orchest-node/Cargo.toml` (vendored OpenSSL on Linux)
- `scripts/copy-node-addon.mjs` (removed once the CLI replaces it)

## Steps

1. Add `@napi-rs/cli`, configure the three targets and generate the
   sub-package directories.
2. Replace the fixed `require` with the loader and its unsupported-platform
   error; cover the error path with a test.
3. Add the Linux-only vendored OpenSSL dependency and confirm CA
   certificates are still found.
4. Update the package metadata.
5. Pack, install into an empty directory and run the tests against the
   installed package.
