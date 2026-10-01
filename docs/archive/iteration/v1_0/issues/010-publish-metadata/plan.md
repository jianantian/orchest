# 010 implementation plan

## Files to read

- `docs/adr/0003-release-policy.md`
- Root `Cargo.toml` and every member `Cargo.toml`

## Files to change

- Root `Cargo.toml` (`[workspace.package]`, `[workspace.dependencies]`)
- Every member `Cargo.toml`
- Root license file(s); per-crate README files for published crates

## Steps

1. Add `[workspace.package]` and `[workspace.dependencies]` entries for the
   internal crates with `path` + `version`.
2. Switch members to inherited fields and workspace dependencies.
3. Add per-crate descriptive metadata and READMEs.
4. Add license file(s) and make sure each published crate packages them.
5. Mark excluded members `publish = false`.
6. Run `cargo publish --workspace --dry-run` and the CI checks.
