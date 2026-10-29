# 007 implementation plan

## Files to read

- `docs/adr/0003-release-policy.md`
- `.github/workflows/release.yml`
- `docs/archive/hotfix/2026_10_01/`（1.1.0 的发布步骤）

## Files to change

- `Cargo.toml`（`[workspace.package].version` 与 `[workspace.dependencies]`）
- `Cargo.lock`、`CHANGELOG.md`、`docs/iteration/roadmap.md`、`AGENTS.md`（版本快照）

## Steps

1. 升版本号，`cargo build --workspace` 更新锁文件。
2. 整理 `CHANGELOG.md`。
3. 跑 workspace 检查与 `cargo publish --dry-run`（逐个 crate）。
4. owner 打 `v1.2.0` tag，确认 release workflow 成功。
5. 更新 roadmap 与 AGENTS 快照。
