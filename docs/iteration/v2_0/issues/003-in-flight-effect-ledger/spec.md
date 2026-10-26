# 003 · in-flight 副作用账本与合成闭包

GitHub issue: #333

## Background

工具调用一旦派发到进程外，副作用是否发生就不再由 runtime 掌控。崩溃后需要回答的问题只有一个：**这个调用到底执行了没有？** 现在的答案是"不知道"，而且 transcript 是线性的，模型看不到任何提示。

业界的三档做法（调研 §3）：

- **显式 unknown（最好）**：dsh 在崩溃后由 agent-loop 合成闭包事件——已记录无结果的调用补 `TOOL_OUTCOME_UNKNOWN`，未记录开始的补 `TOOL_NOT_STARTED`，并给模型明确文案"只对只读/幂等操作重试，否则先查外部状态"（`packages/core/session/src/repair.ts:27-35`）；Kocoro 有副作用台账 `prepared → dispatching → committed / outcome-unknown`，崩溃后注入合成结果、**绝不自动重放**；openhuman 用 `ledger.tool_effects` 记录状态并据此判 unknown。
- **记录但不重发**：Claude Code 恢复时把中断的调用标为 cut off，既不完成也不重跑，并提示模型先确认是否已生效；Codex 未持久化的 item 不回放。
- **缺口**：LangGraph 与 OpenAI Agents 把副作用幂等性完全交给应用。

Pi 没有这一层。Orchest 补上它，是相对 Pi 的结构性差异，而不是功能对齐。

## Goal

让 runtime 精确知道"哪些工具调用已派发但未确认"，并在崩溃恢复时把结论写进日志、让模型看得见，且**任何情况下都不自动重跑**。

## Acceptance Criteria

- [ ] 工具调用在派发前写一条"将派发"记录（调用 id、工具名、参数摘要），派发确认后写"已结算"记录。
- [ ] 账本可从日志投影得出：处于"已派发未结算"的调用集合。
- [ ] 恢复时，对每个未结算调用合成一条闭包记录，结论三选一：**outcome-unknown**（已派发，结果未知）、**not-started**（记录了但未派发）、**aborted-before-dispatch**（因取消未派发）。
- [ ] 合成闭包以正常工具结果的形式进入消息历史（`isError` + 稳定错误码 + 面向模型的说明文案），模型因此知道"先查外部状态再决定是否重试"。
- [ ] **恢复后不自动重跑任何未结算的工具调用**；runtime 不代替应用判断幂等性。
- [ ] 正常路径下账本不产生噪声：已结算的调用不产生合成记录。
- [ ] 取消导致的未派发调用按"aborted-before-dispatch"记账，不混入 unknown。
- [ ] 事件形态与错误码稳定（跨版本可依赖），并写进 `CHANGELOG.md`。
- [ ] 测试覆盖：派发后崩溃 → outcome-unknown；派发前崩溃 → not-started；取消时未派发 → aborted-before-dispatch；正常完成 → 无合成记录。

## Blocked by

001（日志模型）、002（落点与恢复流程）。

## Notes

- 错误码建议：`TOOL_OUTCOME_UNKNOWN`、`TOOL_NOT_STARTED`、`TOOL_ABORTED_BEFORE_DISPATCH`，与 dsh 的命名习惯一致，便于跨系统对照。
- "参数摘要"要避免把密钥写进日志：只记录参数结构与必要标识，不记录凭据值。
- 账本只是 runtime 事实；宿主可以据此展示"这次 Turn 有 N 个副作用结果未知"，但是否继续由宿主与用户决定。
