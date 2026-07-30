# 005 — 实施计划

## 要读的文件

- `docs/iteration/v0_16/prd.md`、001–004 的 `spec.md`/`plan.md` 与本目录 `spec.md`
- `examples/demo/briefing-desk/README.md`（最终 eval 命令和敏感数据规则）
- `examples/demo/briefing-desk/src/harness.rs`（唯一允许修改的 candidate surface）
- `examples/demo/briefing-desk/evals/cases.json` 与 `evals/session-seeds/`
- baseline/candidate 的本地 `manifest.json`、`harness/snapshot.json`、compare reports 和代表性
  trajectories
- `docs/review/` 现有报告（格式、脱敏和 finding 写法参照）

## 要改的文件

- `examples/demo/briefing-desk/src/harness.rs`
  （仅在人工接受 eligible candidate 时保留最终 prompt/description diff）
- `docs/review/v0_16_eval_lab.md`（公开、脱敏的实验与决策报告）
- 必要时新增独立 finding 文档（只记录 runtime gap，不在本 iteration 修改 core）

本地 `examples/demo/briefing-desk/evals/runs/` 只作为实验输入，不加入 git。

## 步骤

1. 在运行 live model 前完成 workspace checks；固定 provider、model、非秘密 request options、
   commit、fixture/case/session-seed revisions，并在实验日志写明。清理或拒绝 harness 外 dirty
   changes。
2. 运行 optimization + validation baseline，先检查 artifact 完整性和 resource coverage，再
   检查全部 baseline must-pass attempts 绝对通过；若返回 `invalid_baseline`，停止 candidate
   eligibility，修复 corpus/grader 或重跑合法 baseline，不能把共同失败视为零回归。
3. 从 baseline optimization 失败和 representative validation trajectory 写第一个人工因果
   假设；只编辑 `harness.rs` 的文本 surface，保存 diff，再运行同配置 candidate。
4. 对 candidate 执行 compare；保留所有 artifact 和失败原因。若仍需迭代，先为下一 candidate
   写新的诊断/假设，最多总计三个 candidate，不调 gate、不改 grader/corpus/runtime。
5. 对每个 eligible candidate 人工查看完整 harness diff、至少一个改善 case 和一个未变/退步
   case；记录接受或拒绝理由。只有接受的候选才允许成为最终 surface。
6. 从任一 run 的 `harness/snapshot.json` 在临时副本中恢复全部 surface，重新计算并核对 manifest
   SHA-256；在报告中记录恢复结果，证明不是只验证 hash 变化。
7. 只有人工接受 eligible candidate 后才运行 `--split scorecard --confirm-sealed`。若查看逐
   case 结果后再改 harness，立即在报告标记失封并说明补充/轮换策略；没有 eligible candidate
   时不运行 scorecard。
8. 编写 `docs/review/v0_16_eval_lab.md`：日期/model/commit、baseline validity、每个预注册假设、
   snapshot/diff 摘要、overall/per-tag/must-pass、token/cost/latency、代表性行为证据、scorecard
   状态、人工决策和 demo-local/抽取结论。
9. 脱敏报告：不复制完整 trajectory、Tool payload、生成报告、API key、authorization/cookie
   或 reasoning；只保留足以审计判断的短 evidence 和聚合数据。
10. 若观察到 runtime correctness gap，只写独立 finding 和复现条件；不修改 `orchest`、
    protocol、provider 或 bindings。
11. 重新运行 workspace test/clippy/fmt/lint-check；确认 `git status` 不包含
    `evals/runs/`，提交最终 harness（如有）与 review 报告。
