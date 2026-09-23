# 013 implementation plan

## Files to read

- `docs/adr/0003-release-policy.md`
- `.github/workflows/ci.yml`
- `CHANGELOG.md`

## Files to change

- `.github/workflows/release.yml` (new)
- `.github/workflows/ci.yml` (make it callable, if reused)
- `WORKFLOW.md`

## Steps

1. Make the CI checks reusable from the release workflow.
2. Add tag/dispatch triggers and the version and changelog guards.
3. Add ordered, skip-if-published `cargo publish` steps, with `--dry-run`
   when `dry_run` is set.
4. Add GitHub Release creation from the changelog section.
5. Run the dry run and document the release procedure.
