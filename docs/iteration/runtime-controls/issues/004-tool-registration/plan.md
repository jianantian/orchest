# 004 · 实施计划

## 先读

- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`、`crates/orchest/src/tool/`、`js/native.d.ts`。
- `docs/guide/sdk-{python,typescript}.md`、`CHANGELOG-SDK.md`、`.github/workflows/sdk.yml`。

## 修改范围

- Node 注册入口与类型、双端示例、集成测试、迁移及发布说明。

## 步骤

1. 对照 Python 实现确认核心工具的构造及权限语义，明确 Node 入参。
2. 复用核心注册与执行路径，补成功和拒绝测试。
3. 跑两端入口及三平台打包验收，整理变化和版本矩阵。
