# 002 — `RunFailed` 结构化失败 kind(gh #242)

## 背景

`RuntimeEvent::RunFailed { error: String }` 只有一个不透明错误串。v0.15 #236 的
`AgentAsTool::child_failure_error` 因此只能靠字符串匹配(`"budget_exceeded"` 前缀、
`"max_steps_reached"` 精确)裁定 ToolError code——运行时措辞一改,分类静默退化为
`SUB_AGENT_RUN_FAILED`(rustdoc 已记录该脆弱性)。

## 目标/范围

1. `RuntimeEvent::RunFailed` 增加 `kind: RunFailureKind` 字段,`#[serde(default)]`(老 payload
   反序列化为默认 variant;事件 wire 到 Py/Node,向后兼容要求与 v0.13 `RunCompleted.stop_reason`
   同款处理,参照 `events.rs:212-216`)。
2. `RunFailureKind` 定义在 events.rs:消费者真正 dispatch 的变体 `BudgetExceeded` /
   `MaxStepsReached`,其余一律 `Other`(默认)。不为没有消费方的分类发明变体(YAGNI)。
3. `actor.rs` 全部 17 个 `RunFailed` 发射点逐一过一遍:明确属于 budget/max_steps 的标上对应
   kind(如 `:612`/`:639` 一带的限额失败),其余 `Other`;error 文本保持不变(文本仍是给
   人/模型读的)。
4. `AgentAsTool::child_failure_error` 改为消费 `kind`:BudgetExceeded → `BUDGET_EXCEEDED`、
   MaxStepsReached → `MAX_STEPS_REACHED`、Other → `SUB_AGENT_RUN_FAILED`;删除字符串匹配;
   rustdoc 脆弱性段落更新为"kind 由运行时结构化提供"。

## 验收标准

- [x] `RunFailed` 带 `kind`,老 payload(无 kind 字段)反序列化为 `Other`;Py/Node wire 透传不受影响
- [x] budget/max_steps 失败路径发出对应 kind(单测断言事件 kind,不只断言 error 文本)
- [x] `child_failure_error` 零字符串匹配;v0.15 既有的三个分类测试(budget/max_steps/generic)保持绿
- [x] 五项检查全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest/src/events.rs`(RunFailed :218-220、serde default 惯例 :212-216);
  `crates/orchest/src/run/actor.rs` 全部 `RuntimeEvent::RunFailed` 发射点(17 处);
  `crates/orchest/src/tool/agent_as_tool.rs`(`child_failure_error` 与 v0.15 测试);
  `crates/orchest-py`/`orchest-node` 的事件透传(确认 additive 字段无碍)
- 改: events.rs(+kind enum/字段/default)→ actor.rs 发射点 → agent_as_tool 分类器
- 测: kind 断言补充到既有限额测试;serde 老 payload 兼容测试;agent_as_tool 分类测试更新
