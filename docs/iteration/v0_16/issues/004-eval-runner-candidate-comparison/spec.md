# 004 — Eval Runner 与 Candidate Comparison

## 背景

有了 corpus、recorder 与 graders 后，还需要一个统一入口负责重复运行、失败隔离、结果聚合和
baseline/candidate 可比性检查。比较器必须执行预注册 gate，但不能替人接受修改。

## 目标/范围

1. 在 Briefing Desk CLI 增加 eval run/compare 能力，支持 label 与
   optimization/validation/scorecard split。
2. optimization 默认每 case 运行一次；validation 与 scorecard 每 case 运行三次。布尔结果以
   多数决定，连续分数取三次均值。
3. compare 前检查 provider、model、request options、fixture revision、case set、split 与重复
   策略一致；不一致时拒绝比较并列出字段。
4. 实现 acceptance gates：must-pass 零回归、validation 总分至少 +5、per-tag 不下降、平均
   token ≤115%、中位 latency ≤130%、无 inconclusive。
5. compare 输出机器可读 JSON 和人可读 Markdown，状态为 `eligible_for_review` 或逐项失败原因；
   不自动编辑 harness、不自动接受 candidate。
6. scorecard run 要求额外显式确认，并在 manifest 标记 sealed execution；流程约束和失封语义
   写入 README。

## 验收标准

- [ ] eval run 可按单个或多个 split 选择 cases，并使用 001 的 validator 在 model call 前校验。
- [ ] optimization、validation、scorecard 的默认重复次数分别为 1、3、3，结果聚合符合合同。
- [ ] 单个 attempt execution failure/inconclusive 不终止其他 case，但最终 compare 不得把它当
      行为 pass。
- [ ] baseline/candidate manifest 不可比时，compare 失败并列出全部不一致字段。
- [ ] must-pass 回归、overall 提升不足、tag 下降、token 超限、latency 超限、inconclusive
      分别有独立 gate 测试。
- [ ] 全部 gate 通过时只输出 `eligible_for_review`，不会自动写回 harness 或删除失败 candidate。
- [ ] scorecard 未传额外确认时在 model call 前失败；确认运行后 manifest 明确记录。
- [ ] provider 无 pricing 时 cost 显示 unknown，但 token gate 正常执行。
- [ ] CLI integration tests 使用 scripted/fake model，断言 artifact layout、退出码和 summary；
      workspace checks 全绿。

## 备注

- 模型变更是独立实验，不提供 `--allow-model-change` 绕过可比性检查。
- scorecard 的 sealed 属性是流程合同，不声称提供安全隔离。

