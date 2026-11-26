# 007 implementation plan

## Files to read

- `docs/iteration/sdk_v0_2/prd.md` (Owner Setup)
- `WORKFLOW.md` (SDK release procedure)
- `docs/adr/0004-sdk-packaging.md`

## Files to change

- `package.json`, `npm/*/package.json`, `crates/orchest-py/Cargo.toml`,
  `crates/orchest-node/Cargo.toml` (version)
- `CHANGELOG-SDK.md` and `.github/workflows/release-sdk.yml`
- `docs/iteration/sdk_v0_2/prd.md`, `docs/iteration/roadmap.md`,
  `AGENTS.md`

## Steps

1. Confirm the owner setup and use Trusted Publishing on both registries.
2. Pass explicit local tarball paths to npm and verify them with
   `npm publish --dry-run`. Bump the SDK to `0.2.0-rc.3` and validate
   the three-platform SDK build.
3. Tag `sdk-v0.2.0-rc.3`, get both publish approvals and check the
   post-publish verification.
4. Get owner sign-off, bump to `0.2.0`, tag `sdk-v0.2.0` and check the
   verification and provenance on both registries.
5. Close out the PRD, the roadmap and `AGENTS.md`.
