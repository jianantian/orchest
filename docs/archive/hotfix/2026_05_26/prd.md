# Hotfix 2026-05-26 PRD：Review 问题清偿 + Lint 防线

## 背景

v0.6 架构改造完成后的全库 code review（[`docs/review/2026-05-25.md`](../../review/2026-05-25.md)）发现 3 个 bug、1 个安全一致性问题、15 个结构债务、2 个可靠性问题，以及测试覆盖缺口。

本轮定位：**降熵重构**。修复存量缺陷、消除结构债务、建立 lint 防线防止回归。不新增对外可见功能。

## 目标

Hotfix 完成后：

1. B1–B3 三个 bug 已修复，不再有 budget 信息丢失、审批永久挂起、PythonSession 死循环
2. S1 安全一致性问题已修复，ExecutePythonTool 与 BareSubprocessExecutor 环境变量模型对齐
3. 全部 15 个结构债务（A1–A15）已修复或有明确的 lint 卡点
4. R1/R2 可靠性问题已修复
5. `clippy.toml` + workspace lint + CI 脚本检查落地，阻止代码质量回归
6. 测试覆盖补齐 compaction、webhook、binding crates

## 成功指标

- `cargo test --workspace` 全绿
- `cargo clippy --workspace -- -D warnings` 全绿（无 `#[allow(clippy::result_large_err)]` 或 `#[allow(clippy::too_many_arguments)]` 残留）
- `clippy.toml` 存在且配置 `too-many-lines-threshold = 200`、`too-many-arguments-threshold = 5`
- CI 脚本检查无违规输出（文件超长、mod.rs 超 50 行、async 中 `std::fs`）
- `crates/` 中无 `#[allow(clippy::` 注解（除非附有不可消除原因的注释）
- `RunHandle` 提供 `abort()` 方法，`CancellationToken` 在 loop 顶部检查
- 全部 Error 类型实现 `std::error::Error::source()`

## 范围

### 在范围内

- 3 个 bug 修复（B1–B3）
- 1 个安全一致性修复（S1）
- 15 个结构债务修复（A1–A15）
- 2 个可靠性修复（R1–R2）
- T1–T9 文档与类型修复
- 测试覆盖补齐
- Lint 与 CI 防线配置

### 不在范围内

- 中间件 / Hook 框架（v0.7 增量能力）
- Loop Detection（v0.7 增量能力）
- Handoff 重构（v0.7 增量能力）
- Session 持久化（v0.7 增量能力）
- 新 Provider（v0.7 增量能力）
- LLM Retry / Circuit Breaker（v0.7 增量能力）

## Issues 拆解

| Issue | 标题 | 范围 |
|-------|------|------|
| [001](./issues/001-bug-fixes/spec.md) | Bug 修复 + 安全一致性 | B1–B3, S1 |
| [002](./issues/002-error-types/spec.md) | Error 类型治理 | A6, A13, T7 |
| [003](./issues/003-micro-fixes/spec.md) | 一行级结构修复 | A8, A10, A12, A14, A15, R2 |
| [004](./issues/004-cancellation/spec.md) | 运行取消机制 | R1 |
| [005](./issues/005-config-validation/spec.md) | AgentConfig 校验 | A7 |
| [006](./issues/006-dependency-inversion/spec.md) | 依赖反转 | A1 |
| [007](./issues/007-provider-cleanup/spec.md) | Provider 层去重与注册表 | A2, A3, A4, A5, A9, A11 |
| [008](./issues/008-test-coverage/spec.md) | 测试覆盖补齐 + 文档修复 | 测试缺口, T1–T9 |
| [009](./issues/009-lint-ci/spec.md) | Lint 配置 + CI 防线 | clippy.toml, workspace lint, CI 脚本 |

## 依赖关系与执行顺序

```
001 (bug/安全)     ─── 无依赖，立即开始
002 (error 类型)   ─── 无依赖，可与 001 并行
003 (一行级修复)   ─── 无依赖，可与 001/002 并行
004 (取消机制)     ─── 无依赖，可与 001–003 并行
005 (config 校验)  ─── 无依赖，可与 001–004 并行

006 (依赖反转)     ─── 建议在 002 之后（Error 类型稳定后再拆 crate）
007 (provider 清理) ── 必须在 006 之后（ProviderFactory 依赖 ModelAdapter 路径），建议也在 002 之后

008 (测试 + 文档)  ─── 在 001–007 全部完成后推进
009 (lint + CI)    ─── 在 001–007 全部完成后落地（确认无 clippy 违规后锁定）
```

001–005 可全部并行。006→007 有强依赖（007 的 `ProviderFactory` trait 返回 `Box<dyn ModelAdapter>`，`ModelAdapter` 在 006 后位于 `agent-runtime-model`）。008/009 收尾。

## 权威顺序

1. `docs/hotfix/2026_05_26/issues/*/spec.md` 是实施与验收的第一权威
2. 本 PRD 约束范围和优先级；与 issue 细节冲突时，以 issue spec 为准
3. `docs/review/2026-05-25.md` 是问题来源参考
