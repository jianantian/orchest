# 003 · 实施计划

## 先读

- `crates/orchest/src/session/{store,sqlite,log}.rs`、`crates/orchest/src/run/{mod,config,checkpoint_tests}.rs`。
- 绑定 Cargo feature、打包工作流及两端现有配置入口。

## 修改范围

- Rust 会话层的 SQLite 写入协调、绑定配置、运行对象启动分支、类型与会话恢复示例。

## 步骤

1. 确认 SQLite feature 与三平台打包。定义存储和会话 ID 的显式配置及稳定错误映射。
2. 在 Rust 会话层统一协调数据库身份与 Session ID。用跨进程 fixture 验证写入权的取得、释放和异常退出后的接管。覆盖同一数据库的不同连接，避免为了持有单个会话的写入权而锁住整个数据库。
3. 委托核心恢复入口，区分原样恢复和追加新轮次。用临时数据库测隔离、checkpoint、副作用规则、固定长度日志的恢复延迟和峰值内存。
