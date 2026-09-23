# 010 · Publish metadata and license

GitHub issue: #307

## Background

Workspace crates declare only `name`, `version` and `edition`, and depend on
each other through bare `path` dependencies. crates.io rejects packages
without a license or description and rejects path dependencies without a
`version`. The repository has no license file.

## Goal

Make every crate the release policy marks as published packageable, with
shared metadata defined once at the workspace root.

## Acceptance Criteria

- [ ] The root `Cargo.toml` defines `[workspace.package]` with at least
  `version`, `edition`, `license`, `repository` and `rust-version` per the
  ADR, and published crates inherit them with `.workspace = true`.
- [ ] Each published crate has a `description`, `readme`, up to five
  `keywords` and valid crates.io `categories`, and its README exists.
- [ ] Every intra-workspace dependency of a published crate carries a
  `version` (via `[workspace.dependencies]`) matching the workspace version.
- [ ] Every member the ADR excludes has `publish = false`.
- [ ] `orchest-provider` depends on each Internal-tier crate with an exact
  `=` version requirement, and each Internal-tier crate's crate-level docs
  state it is not for direct use (ADR-0003 D2).
- [ ] The license file(s) exist at the repository root and appear in
  `cargo package --list` for each published crate.
- [ ] `cargo publish --workspace --dry-run` succeeds.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
  `cargo fmt --check` and the CI `cargo doc` step still pass.

## Blocked by

- #306 (release policy)
