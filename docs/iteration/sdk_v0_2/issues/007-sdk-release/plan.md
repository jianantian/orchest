# 007 implementation plan

## Files to read

- `docs/iteration/sdk_v0_2/prd.md` (Owner Setup)
- `WORKFLOW.md` (SDK release procedure)
- `docs/adr/0004-sdk-packaging.md`

## Files to change

- `package.json`, `npm/*/package.json`, `crates/orchest-py/Cargo.toml`,
  `crates/orchest-node/Cargo.toml` (version)
- `CHANGELOG-SDK.md`
- `docs/iteration/sdk_v0_2/prd.md`, `docs/iteration/roadmap.md`,
  `AGENTS.md`

## Steps

1. Confirm the owner setup for the release candidate is done.
2. Bump to `0.2.0-rc.1`, tag `sdk-v0.2.0-rc.1`, get the publish approvals
   and check the post-publish verification.
3. Have the owner configure npm trusted publishers and delete the token.
4. Get owner sign-off, bump to `0.2.0`, tag `sdk-v0.2.0` and check the
   verification and the provenance on both registries.
5. Close out the PRD, the roadmap and `AGENTS.md`.
