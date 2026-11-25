# 003 实施计划

## 阅读

- [002 规格](../002-input-reconstruction/spec.md)
- `crates/orchest/src/run/actor.rs` 中模型阶段、流转发、hook 与 assistant 入账路径
- `crates/orchest/src/session/log.rs` 的消息与工具账本事件

## 预计修改

`crates/orchest/src/run/actor.rs`，`crates/orchest/src/session/` 的独立证据 schema 与读取，模型流和 hook 的现有测试。

## 步骤

1. 复用尝试标识，分别记录 adapter 结算及可确认的发送事实。沿已有流转发点以有界成本观察允许的片段，保持转发顺序和等待条件。
2. 在两个 assistant append 位置取实际事件序号，使用 001 的事件确认机制。工具和恢复结果引用 v2 账本原义。
3. 扩展 mock 对照场景，覆盖改写、拒绝、异常 stop、流失败、崩溃与序号复用、派发前 checkpoint 失败和截断。验证运行时观察流与原有副作用边界。
