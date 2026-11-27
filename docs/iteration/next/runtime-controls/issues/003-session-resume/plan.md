# 003 · 实施计划

## 先读

- `crates/orchest/src/session/{store,sqlite,log}.rs`、`crates/orchest/src/run/{mod,config,checkpoint_tests}.rs`。
- 绑定 Cargo feature、打包工作流及两端现有配置入口。

## 修改范围

- 绑定配置、存储接线、运行对象启动分支、类型与会话恢复示例。

## 步骤

1. 确认 SQLite feature 与三平台打包。定义存储和会话 ID 的显式配置及稳定错误映射。
2. 委托核心恢复入口，区分原样恢复和追加新轮次。
3. 在临时数据库测试实例重建、隔离、错误、checkpoint 和副作用规则。
