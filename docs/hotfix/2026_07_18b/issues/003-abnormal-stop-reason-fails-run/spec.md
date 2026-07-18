# 003 — 异常 stop_reason 终止 run,而非空消息死循环

## 背景

模型返回非 `EndTurn`/`MaxTokens` 的 stop_reason(如 `ContextWindowExceeded`)且无 tool_use 时,`run_one_step` 不终止 run,而是落入工具阶段;`apply_tool_phase_results` push 一条**空 content 的 User 消息**(`crates/orchest/src/run/actor.rs:1530-1544`),`step += 1` 后用相同上下文再次调用模型——循环直到 `max_steps`(默认 100)耗尽。烧掉大量 token 后 run 仍失败,且失败原因被稀释。

## 目标/范围

该分支直接终止 run:

- 发 `RunFailed`,错误信息带 stop_reason;
- 不 push 空 User 消息,不再消耗后续 step;
- 终止前走 `on_run_error` hook(与 `check_step_limits` 的失败路径一致,`actor.rs:591` 附近)。

有 tool_use 时行为不变(正常工具分发);`EndTurn`/`MaxTokens` 路径行为不变(`MaxTokens` 截断标记是 SDK-B2,留后续迭代,不在本 hotfix)。

## 验收标准

- [ ] model stub 返回异常 stop_reason 且无 tool_use:run 立即 `RunFailed`,错误含 stop_reason,messages 中无空 User 消息,且只调用模型一次
- [ ] 异常 stop_reason 但有 tool_use:正常走工具分发
- [ ] `EndTurn`/`MaxTokens` 完成路径行为不变(现有测试不回归)
- [ ] `on_run_error` hook 被调用(与限额失败路径行为一致)
- [ ] 新增测试;四件套全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest/src/run/actor.rs`(`run_one_step` 的 stop_reason 分桶,`actor.rs:953-979`;`apply_tool_phase_results`;`on_run_error` hook 调用点)
- 改: `run_one_step` 的响应分桶处新增异常分支;`apply_tool_phase_results` 视需要加防御
- 测: 用现有 model stub/test harness 新增用例
