# 005 — 实施计划

## 要读的文件

- `crates/orchest-provider-http/src/chat.rs`(`build_request_body` 全部分支、OptionAdjustment 用法)
- `crates/orchest/src/run/compaction.rs`(切分逻辑 :40-120 与现有单测)
- `crates/orchest/src/run/actor.rs`(并行回插 :1063-1066;串行/handoff :1593-1650)
- `crates/orchest-provider-http/src/messages.rs`(`Role::Tool` 的 wire 处理)
- `docs/polaris/observability.md`(adjustment/警告惯例)

## 要改的文件

- `crates/orchest-provider-http/src/chat.rs`(C5)
- `crates/orchest/src/run/compaction.rs`(C6)
- `crates/orchest/src/run/actor.rs` + 受影响的快照/hook 测试(C7)
- 迭代实现记录(migration 说明)
- 测试

## 步骤

1. C5:三个分支逐一核,丢弃点补 OptionAdjustment + warn;单测断言 adjustment 出现。
2. C6:切分点对齐算法(向前/向后walk到安全边界)+ 孤儿 tool_result 单测。
3. C7:并行路径改 Role::User;messages.rs Role::Tool 处理核查;受影响测试更新;migration 说明落字。
4. 五项检查。
