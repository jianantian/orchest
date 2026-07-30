# 003 — 实施计划

## 要读的文件

- `docs/iteration/v0_16/prd.md`、001/002 的 `spec.md`/`plan.md` 与本目录 `spec.md`
- `examples/demo/briefing-desk/src/eval/case.rs`
  （case、grader、Tool/order/fact/report 合同）
- `examples/demo/briefing-desk/src/eval/trajectory.rs`、`artifact.rs`
  （grader 可见 schema、attempt status/output）
- `examples/demo/briefing-desk/fixtures/`（合法 citation root 与 42%/35% 来源）
- `examples/demo/briefing-desk/evals/cases.json`（18 个真实 grader 配置）

## 要改的文件

- `examples/demo/briefing-desk/src/eval/mod.rs`
- `examples/demo/briefing-desk/src/eval/grader/mod.rs`
  （统一 grader 输入/输出与 dispatch）
- `examples/demo/briefing-desk/src/eval/grader/tool_flow.rs`
  （selection、partial-order、follow-up grounding）
- `examples/demo/briefing-desk/src/eval/grader/content.rs`
  （modality、conflict、report、citation）
- `examples/demo/briefing-desk/src/eval/grader/aggregate.rs`
  （attempt/case/tag/split 公式与 must-pass）
- `examples/demo/briefing-desk/evals/test-fixtures/`
  （脱敏 trajectory/output pass/fail/boundary fixtures）
- `examples/demo/briefing-desk/src/eval/grader/` 内单元测试

## 步骤

1. 先定义并测试统一 `GraderResult`：grader/case ID、`passed`、0–100 score、正权重、evidence、
   failure reason；未知 grader、越界 score、零/负权重和 grader error 都是结构化错误。
2. 以固定 trajectory fixtures 写 Tool selection 测试：required/forbidden/optional、实际调用
   序列、failed/retried calls 的计数语义明确；再实现 grader。
3. 为 Tool chaining 写 partial-order 测试：只约束声明的前后边，不把无关 Tool 强制成全序；
   覆盖缺前置读取、写入过早、retry 后成功和 forbidden chain。
4. 为 modality/conflict 写测试：音频/图片由 canonical Tool events 证明，文本由 fixture read
   证明；42%/35% 必须同时出现在 output 并与正确来源关联，不能只做裸字符串命中。
5. 为 report/citation 写测试：要求 section/事实、引用存在于 fixture inventory、canonicalize
   后仍位于 fixture root，拒绝不存在路径、`..` 越界和正文提名但未形成来源记录。
6. 为 follow-up grounding 写测试：attempt metadata 的 seed/resume 状态有效，历史事实出现在
   answer，且 case 禁止的 search/read/full-chain Tool 未重新执行；缺 seed、fresh run 或重跑
   材料链均失败。
7. 实现 attempt 聚合并固定公式：completed + 全部 required grader pass 才 pass，score 为
   grader-weighted mean；grader error 或缺结果返回 null aggregate。
8. 实现 repetition 聚合：普通三次 case 至少 2/3 pass、score 取三次均值；must-pass 要求每次
   pass。任一必需 attempt 非 completed/grading completed 时不缩小分母，case aggregate 为
   null。
9. 实现 case-weighted overall/per-tag；validation 中任一注册 tag 无 case 时返回 corpus error。
   测试 must-pass 不能被高分抵消、共同失败不构成通过、multi-tag case 的权重口径一致。
10. 直接对 18 个提交 case 跑 grader contract validation，确认七类标签都有可执行的确定性
    grader，不引入 model/embedding/network。
11. 运行 demo tests 与 workspace test/clippy/fmt/lint-check。
