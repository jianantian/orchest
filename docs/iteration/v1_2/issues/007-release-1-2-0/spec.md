# 007 · 发布 1.2.0

GitHub issue: #348

## Background

001–006 都是增量变更。SDK 0.2 要给出 SDK↔crate 版本矩阵（ADR-0005 D6），矩阵里的 crate 版本应当是已发布版本，而不是 main 上的某个提交。

## Goal

八个 crate 以 `1.2.0` 统一版本发布到 crates.io。

## Acceptance Criteria

- [ ] workspace 版本与内部依赖要求升到 `1.2.0`（ADR-0003 lockstep）。
- [ ] `CHANGELOG.md` 的 `## [1.2.0]` 列出 001–006 的新增项，无 Breaking Changes。
- [ ] `v1.2.0` tag 触发 release workflow，八个 crate 发布成功。
- [ ] `docs/iteration/roadmap.md` 把 v1.2 移到「已完成」。

## Blocked by

001–006。

## Notes

- HITL：tag 与 crates.io 发布需要 owner 操作或审批。
