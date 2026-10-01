# 014 implementation plan

## Files to read

- `docs/iteration/v1_0/prd.md`
- `docs/adr/0003-release-policy.md`
- `WORKFLOW.md` release section

## Files to change

- Root `Cargo.toml` (version)
- `CHANGELOG.md`
- `docs/iteration/v1_0/prd.md`, `docs/iteration/roadmap.md`

## Steps

1. Bump to `1.0.0-rc.1`, update the changelog and run the full CI suite.
2. Regenerate the public API inventory and confirm no diff.
3. Tag and publish the candidate; smoke-test it from a scratch project.
4. Pre-public review (ADR-0003 D8) is settled: `docs/external/` stays and
   the `docs/analysis` history may be public. Make the repository public.
5. Get owner sign-off, bump to `1.0.0` and tag.
6. Confirm crates.io and the GitHub Release, then close out the PRD and roadmap.
