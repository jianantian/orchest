# 005 — 人工 Harness 实验与验证报告

## 背景

前四个 issue 只证明 Eval Lab 的部件正确。本 issue 必须用同一个 live chat model 运行真实
baseline/candidate，验证这些部件能否支持一次可审计的 harness engineering 决策。iteration 的
成功标准是闭环可信，不是强制找到分数更高的 prompt。

## 目标/范围

1. 固定一个 live chat provider/model/options，运行 optimization + validation baseline。
2. 人工提出最多三个候选。每个候选先写清失败诊断、因果假设和预期改善，再只修改
   `harness` 模块中的 prompt/description。
3. 对每个候选运行相同 optimization + validation，并生成 compare report；保留失败候选，不
   覆盖或挑选性删除结果。
4. 若出现 `eligible_for_review` candidate，人工审阅 diff 与代表性 before/after trajectories，
   记录接受或拒绝理由；只有接受后才运行 sealed scorecard。
5. 产出 `docs/review/v0_16_eval_lab.md`，公开脱敏后的配置、假设、diff 摘要、分数、token、
   latency、回归、scorecard 状态和最终结论。

## 验收标准

- [x] baseline 与所有 candidates 使用相同 provider、model、request options、fixtures、
      cases、session seed hashes 和 repetition policy；报告记录 provider、model、日期、commit
      与 harness/effective-config snapshot hashes，且 effective-config hashes 相同。
- [x] 至少一个人工 candidate 被完整运行；最多三个，且每个在运行前有书面因果假设。
- [x] 每个 candidate 只修改 `harness` 模块；若实验发现必须改 runtime，记录独立 finding，
      不在 v0.16 实现。
- [x] 每个 candidate 都有 compare report；失败或 inconclusive 结果如实保留。
- [x] baseline must-pass 绝对有效；若无效则停止候选资格判断、记录 `invalid_baseline` 并修复
      corpus/grader 或重跑基线，不把共同失败解释为零回归。
- [x] eligible candidate 的人工 review 同时查看 diff 与至少一个改善、一个未变化或退步 case
      的 trajectory。
- [x] 只有人工接受的 eligible candidate 运行 scorecard；没有 eligible candidate 时不运行，
      iteration 仍可完成。
- [x] 若 scorecard 被查看后继续修改，报告明确标记失封，并说明是否补充/轮换案例。
- [x] 验证报告不包含 API key、authorization、完整 Tool payload、完整生成文本或 hidden
      reasoning；本地敏感 artifacts 不提交。
- [x] 报告从 `harness/snapshot.json` 实际恢复一次 surface，并核对 harness/effective-config
      两组 snapshot/hash；明确回答闭环是否可运行、grader 是否可信、manifest + snapshots 是否
      足以复现、是否找到有效 harness 改进、下一步应保持 demo-local 还是值得抽取。
- [x] 最终 workspace checks 全绿。

## 备注

- live ASR/TTS 不是验收项；public fakes 可继续使用。
- “三个候选均未改善”是合法实验结果，不得为了让 iteration 看起来成功而放宽 gate 或改写分数。
