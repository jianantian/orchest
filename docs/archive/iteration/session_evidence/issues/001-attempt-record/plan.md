# 001 实施计划

## 阅读

- [迭代 PRD](../../prd.md)
- [Session v2 checkpoint 契约](../../../../../iteration/v2_0/issues/002-checkpoint-and-resume/spec.md)
- `crates/orchest/src/run/actor.rs`、`crates/orchest/src/session/{store,log}.rs`

## 预计修改

`crates/orchest/src/session/` 的独立证据存储与读取入口，`crates/orchest/src/run/` 的 Rust 配置、actor 调用边界和测试。

## 步骤

1. 定义独立 schema、尝试标识、会话作用域、重复写入、读取覆盖与失效语义。使用独立的 SQLite 证据库，通过可选 `session-evidence` feature 启用。首个 `RunStarted.run_id` 标识会话实例，宿主每个 SessionStore 使用独立证据库并显式绑定，实现有界的异步写入、分页读取、删除及版本校验。
2. 在被选中的 dispatch 分支真正进入 adapter 调用时分配尝试身份，记录本地拒绝和可确认结算。调用前取消归入本地决策。通过增量 Rust 配置接入，保持现有配置序列化和必实现 trait 方法。
3. 在原有 append 与 checkpoint 路径提供共用的事件引用确认机制。原耐久屏障成功时才为这批实际追加的事件登记确认，不等待证据存储完成。用合成事件验证旧证据遇到复用序号时仍未确认。
4. 验证 retry、外部 resume 与 Supervisor 重启的身份，检查同名 Session 隔离、重复写、重开和部分写入。用受控阻塞写端制造饱和与丢失，并对照 v2 恢复和工具执行结果。
