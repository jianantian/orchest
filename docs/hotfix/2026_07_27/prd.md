# Hotfix 2026_07_27 — v0.15 评审跟进(#240 / #242 / #241)

> 来源: v0.15 迭代 code review 发现的三项跟进,均已建 GitHub issue。
> 三项小、独立、修复收益立竿见影,按 hotfix 档处理。

## 背景

1. **#240 trace id 碰撞**: `new_trace_id()`(`crates/orchest-provider-core/src/telemetry.rs:57-66`)用
   `SystemTime` 纳秒 XOR 栈地址生成 id;macOS 时钟 µs 粒度 + 同一调用点栈地址相同 → 连续两次调用
   可产生相同 id。这既是测试 flake(全量跑 4 次挂 2 次),也是**生产缺陷**(trace id 本用于关联,
   碰撞即失联)。
2. **#242 RunFailed 失败分类靠字符串匹配**: `AgentAsTool::child_failure_error`(v0.15 #236)靠
   `"budget_exceeded"` 前缀 / `"max_steps_reached"` 精确匹配运行时失败消息裁定 code;运行时措辞一改,
   分类静默退化为 `SUB_AGENT_RUN_FAILED`。
3. **#241 失败子代理 budget 不折算**: v0.15 #236 后,child 失败路径返回 `Err(ToolError)`,不产生
   `ToolOutput`;`BudgetGuard` 只经 `Ok(Structured).external_usage` 折算 child 消耗 → 父模型反复调用
   持续失败的子代理时,child 花费不计入父预算,仅剩 `max_steps` 兜底。

## 范围与依赖顺序

| # | Issue | gh | 文件 |
|---|-------|----|------|
| 001 | trace id 生成混入单调计数器 | #240 | `issues/001-trace-id-monotonic-counter/spec.md` |
| 002 | `RunFailed` 结构化失败 kind,agent_as_tool 改消费 | #242 | `issues/002-runfailed-structured-kind/spec.md` |
| 003 | `ToolError` 携带 external_usage,actor Err 路径折算 | #241 | `issues/003-failed-subagent-budget-fold/spec.md` |

001 独立;002 先于 003(两者都改 `agent_as_tool.rs` 的失败路径,002 先改写分类器);按编号串行提交,一项一 commit。

## 验收

- `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh` / `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` 全绿
- 各 issue spec.md 验收框全勾

## Out of scope

- trace id 换 uuid crate(无依赖原则,计数器方案已足够)
- `SubAgentFailed` 事件加 `budget_used` 字段(003 选了 ToolError payload 路线,事件路线不做)
- v0.15 评审的 Minor 建议项(expect_output 绑定暴露等,已记计划文档)
