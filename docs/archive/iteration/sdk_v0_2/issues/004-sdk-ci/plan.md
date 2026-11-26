# 004 implementation plan

## Files to read

- `.github/workflows/ci.yml`
- `pyproject.toml`, `package.json`
- `docs/adr/0004-sdk-packaging.md`

## Files to change

- `.github/workflows/sdk.yml` (new)
- `WORKFLOW.md` (when the job runs)

## Steps

1. Add the platform matrix with native runners and `maturin-action` for
   the wheels (manylinux_2_28 on Linux).
2. Add the Node addon build and packing for each platform.
3. Add the install-and-test steps, with the extra Python and Node versions
   on Linux x86_64.
4. Upload artifacts and add the `push`, path-filtered `pull_request` and
   `workflow_call` triggers.
5. Confirm a docs-only pull request does not start the job.
