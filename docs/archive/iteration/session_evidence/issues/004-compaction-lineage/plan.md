# 004 实施计划

## 阅读

- [002 规格](../002-input-reconstruction/spec.md) 与 PRD 的事件耐久关联规则
- `crates/orchest/src/run/{actor,compaction}.rs` 与 `crates/orchest/src/session/log.rs`
- [DSH 压缩证据和投影](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/docs/subsystems/compaction.md#L7-L21)

## 预计修改

`crates/orchest/src/run/{actor,compaction}.rs` 的压缩观察点，`crates/orchest/src/session/` 的证据关联和读取，压缩与恢复测试。

## 步骤

1. 在已有压缩路径记录触发、跳过原因、摘要调用及结算。摘要调用复用 001 的尝试身份与 002 的输入采集。沿当前切分与保留逻辑传递已知来源，任意 hook 改写后缺少证据的段标为未知。
2. 按实际 `Compacted` append 和原有 checkpoint 更新替换与耐久状态。分别处理摘要失败、摘要跳过和实际提交的 hook 改写。让下一次有效输入关联其 Session 基线与实际 adapter 输入。
3. 验证连续压缩、提前返回、失败、取消、同正文不同来源、handoff、工具配对和证据写入失败。对照 v2 恢复及 mock adapter 的独立摘要输入和后续主循环输入。
