# 003 — 失败子代理 budget 折算回父 BudgetGuard(gh #241)

## 背景

v0.15 #236 把 agent-as-tool child 失败从 `Ok(Structured{details.error})` 改为 `Err(ToolError)`——
语义正确,但失败路径不再产生 `ToolOutput`,而 `BudgetGuard` 只经 `Ok(Structured).external_usage`
折算 child 消耗(`crates/orchest/src/run/actor.rs` `record_external_usage` 调用点)。基线上失败路径
也带 `external_usage`,故这是核算回退:父模型反复调用持续失败的子代理时,child 花费不计入父预算,
仅剩 `max_steps` 兜底(每个 child 仍被 `cap_budget` 单独限制)。rustdoc/spec 已记录后果,本 issue
做机制修复。

## 目标/范围

1. `ToolError` 增加可选诊断字段 `external_usage: Option<BudgetUsage>`(`#[serde(default)]`,
   向后兼容;`BudgetUsage` 已 Serialize/Deserialize)+ `with_external_usage()` builder;
   各构造函数补字段初始化。全 workspace 搜 `ToolError {` 字面量构造点一并更新。
2. actor 工具执行 **Err 路径**(串行 + 并行两条):`err.external_usage` 存在时同样
   `record_external_usage` 折算,语义与成功路径一致。
3. `agent_as_tool`: `child_failure_error` 与输出契约违规错误(002 落地后的现状)都附上
   `external_usage`(契约违规为两次 attempt 的合计)。
4. `AgentAsTool` rustdoc 的"budget accounting consequence"段落更新:不再是"不折算",改为
   "经 `ToolError.external_usage` 折算,与成功路径一致";003 spec(v0.15)备注指向本 hotfix。

## 验收标准

- [ ] child 失败时父 BudgetGuard 计入 child 已耗 budget(单测:stub child 失败 × N 次,父预算按
  合计递减;budget 耗尽时父 run 按既有 budget_exceeded 语义失败)
- [ ] 输出契约违规(两次 attempt)折算合计 usage
- [ ] `external_usage` 未设置的工具 Err 路径行为不变
- [ ] ToolError serde 向后兼容(老 payload 无该字段可反序列化)
- [ ] 五项检查全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest/src/tool/error.rs`(ToolError 构造器);`crates/orchest/src/run/actor.rs`
  (`record_external_usage` 成功路径调用点 + 工具 Err 处理两条路径);`crates/orchest/src/budget/`
  (BudgetGuard/BudgetUsage);`crates/orchest/src/tool/agent_as_tool.rs`(002 后的失败构造点)
- 改: error.rs(+字段/builder)→ actor.rs Err 路径折算(串行+并行)→ agent_as_tool 附 usage →
  rustdoc 更新
- 测: 父预算折算单测(agent_as_tool 层 + actor 层各一);serde 兼容测试;既有 Err 路径回归
