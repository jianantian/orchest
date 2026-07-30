# 004 — Eval Runner 与 Candidate Comparison

## 背景

有了 corpus、recorder 与 graders 后，还需要一个统一入口负责重复运行、失败隔离、结果聚合和
baseline/candidate 可比性检查。比较器必须执行预注册 gate，但不能替人接受修改。

## 目标/范围

1. 在 Briefing Desk CLI 增加 eval run/compare 能力，支持 label 与
   optimization/validation/scorecard split。
2. optimization 默认每 case 运行一次；validation 与 scorecard 每 case 运行三次，并执行 003
   固定的普通/must-pass 聚合公式。
3. compare 前检查 source git commit、effective-config hash、fixture revision、session seed
   hashes、case set、split 与重复策略一致；不一致时拒绝比较并列出字段。harness snapshot 是
   唯一允许不同的执行输入。
4. 先校验 baseline must-pass 绝对通过，再实现 candidate acceptance gates：candidate
   must-pass 绝对通过、validation 总分至少 +5、per-tag 不下降、平均 gate total tokens
   ≤115%、中位 wall latency ≤130%、无非 completed/resource incomplete。
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
- [ ] baseline 任一 must-pass attempt 失败时返回 `invalid_baseline` 并列出 case/attempt，
      不计算 candidate eligibility。
- [ ] candidate 每个 must-pass attempt 都必须绝对 pass；baseline/candidate 同时失败不能通过。
- [ ] grader score/pass、case repetition、overall 与 per-tag 聚合完全使用 003 合同。
- [ ] `gate_total_tokens` 精确等于 input、output、audio/image/video input 五项之和；
      reasoning/cache/details 只报告不重复相加。
- [ ] compare 要求 effective-config hash 相同；runtime、retry、approval、Tool schema、
      ASR/TTS/vision route 或 session mode 不同都列为不可比，不能伪装成 harness candidate。
- [ ] compare 从磁盘重算两边 effective-config hash；hash/文件不一致时拒绝，配置不同时报告
      字段级 diff，而不是只显示两个 hash。
- [ ] validation mean tokens 以全部 validation attempts 等权计算；latency 从 start/resume 前到
      terminal + handle wait 完成，用 monotonic clock，偶数 median 取中间两项平均。
- [ ] 失败 attempt 记录但不进入 latency median，同时完整性 gate 必须失败；不能通过排除慢失败
      获利。
- [ ] normal、child 与应用内部 vision model usage 全覆盖；缺 usage 标为
      `resource_coverage=incomplete` 并拒绝 eligibility。
- [ ] invalid baseline、candidate must-pass、overall 提升不足、tag 下降、token 超限、latency
      超限、inconclusive 和 resource incomplete 分别有独立 gate 测试。
- [ ] 全部 gate 通过时只输出 `eligible_for_review`，不会自动写回 harness 或删除失败 candidate。
- [ ] scorecard 未传额外确认时在 model call 前失败；确认运行后 manifest 明确记录。
- [ ] provider 无 pricing 时 cost 显示 unknown，但 token gate 正常执行。
- [ ] CLI integration tests 使用 scripted/fake model，断言 artifact layout、退出码和 summary；
      workspace checks 全绿。

## 备注

- 模型变更是独立实验，不提供 `--allow-model-change` 绕过可比性检查。
- scorecard 的 sealed 属性是流程合同，不声称提供安全隔离。
