# 001 · 实施计划

## 先读

- [Session 证据 PRD](../../../../archive/iteration/session_evidence/prd.md)与[运行控制 003](../../../../archive/iteration/runtime-controls/issues/003-session-resume/spec.md)。
- `crates/orchest/src/session/evidence/{binding,records}.rs`、`crates/orchest/src/run/config.rs` 及现有证据回归。
- `crates/orchest-py/`、`crates/orchest-node/` 中的绑定入口、Cargo feature、类型声明和测试，以及 `.github/workflows/{sdk,release-sdk}.yml`。

## 修改范围

- 两个绑定 crate 的 feature、运行配置、读取入口及公开类型。共用元数据投影由 Rust 证据层提供。
- 两端合约测试、SDK 安装验证，以及 [Python 指南](../../../../guide/sdk-python.md)和 [TypeScript 指南](../../../../guide/sdk-typescript.md)。

## 步骤

1. 在 003 完成后，验证两端打包支持核心证据 feature，并将逐次运行的显式配置接到现有会话实例绑定和 `AgentConfig`。验证新建和恢复的默认行为。
2. 在 Rust 证据层固定读取实例，统一检查实例、游标、限额和撤销。复用按页读取通路，投影允许的记录，保持游标推进。两端映射同一组错误与未知状态。
3. 用相同的本地数据库场景验证两端，覆盖重试、空页继续、跨实例游标、双向实例隔离、撤销、含模拟授权内容的旧库和证据故障下的恢复。
4. 对比开关采集时的运行延迟与队列峰值，测量固定页大小下的扫描量、延迟和峰值内存。从本地构建产物验证两端公开 API，更新指南与迭代验收结果。
