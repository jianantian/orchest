# 005 implementation plan

## Files to read

- `.github/workflows/release.yml`, `.github/workflows/sdk.yml`
- `scripts/release-check.sh`, `scripts/release-notes.sh`
- `docs/adr/0004-sdk-packaging.md`

## Files to change

- `scripts/sdk-release-check.sh` (new)
- `CHANGELOG-SDK.md` (new)
- `.github/workflows/release-sdk.yml` (new)
- `scripts/release-notes.sh` (accept a changelog path, if reused)
- `WORKFLOW.md` (SDK release procedure and owner setup)

## Steps

1. Write the version check script and `CHANGELOG-SDK.md`.
2. Add the workflow triggers, the guard job and the call into the SDK job.
3. Add the PyPI and npm publish jobs with approval environments,
   skip-if-published checks and the token-or-OIDC switch for npm.
4. Add the GitHub Release job and the post-publish verification job.
5. Document the procedure, then run the dry run once the workflow is on
   `main`.
