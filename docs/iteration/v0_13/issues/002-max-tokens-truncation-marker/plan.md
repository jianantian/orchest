# 002 — 实施计划

## 要读的文件

- `crates/orchest/src/run/actor.rs`(EndTurn/MaxTokens 完成分支,hotfix 后的现状)
- `crates/orchest/src/events.rs`(`RuntimeEvent::RunCompleted` 形状与命名惯例)
- `crates/orchest-protocol/src/response.rs`(StopReason 定义)
- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`(事件映射/透传方式)
- 事件 serde 相关现有测试

## 要改的文件

- `crates/orchest/src/events.rs`(字段新增)
- `crates/orchest/src/run/actor.rs`(填充字段)
- `crates/orchest-py` / `crates/orchest-node`(透传)
- 测试文件

## 步骤

1. 确定字段形态(`stop_reason: StopReason` vs `truncated: bool`),在 commit body 说明取舍。
2. `RuntimeEvent::RunCompleted` 加字段,`#[serde(default)]` 保兼容;更新所有构造点与模式匹配点。
3. actor 两个完成分支分别填充。
4. Py/Node 事件映射透传。
5. 测试:MaxTokens 与 EndTurn 的事件断言;旧 JSON 反序列化断言。
6. 四件套 + cargo doc。
