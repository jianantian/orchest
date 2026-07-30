# 003 — Deterministic Graders

## 背景

第一版 Eval Lab 的首要风险不是 agent 不够聪明，而是评分信号不可信。可从 trajectory 和报告中
确定观察的行为应使用普通程序评价；若同时引入 LLM judge，会把 agent 随机性和 judge 随机性
混在一起，难以判断 harness 修改是否有效。

## 目标/范围

1. 建立统一 grader 输出：grader ID、case ID、pass/fail、0–100 子分、权重、可读 evidence 与
   failure reason。
2. 实现 Tool selection grader：要求/禁止的 Tool 是否按 case 合同出现。
3. 实现 Tool chaining grader：顺序约束、必要的前置读取与最后写入/合成行为。
4. 实现 modality/conflict grader：文本、音频、图片覆盖，以及 42%/35% 冲突的出现与来源归因。
5. 实现 report/citation grader：要求 section、关键事实和 fixture 引用存在且引用目标真实。
6. 实现 follow-up grounding grader：follow-up 使用持久化 session，且没有重新执行被 case
   禁止的完整材料工具链。
7. 固定 grader → attempt → case → tag/split 的聚合公式；must-pass 结果单独呈现，不被平均分
   隐藏。

## 验收标准

- [ ] 七类行为标签均至少有一个确定性 grader 消费。
- [ ] grader 只读取 case contract、trajectory、output 与 fixture inventory，不发起 model call。
- [ ] Tool selection 能区分 required、forbidden、optional，并在失败时列出实际调用序列。
- [ ] Tool chaining 支持部分顺序约束，不要求无关 Tool 的全序一致。
- [ ] modality/conflict grader 能证明音频和图片 Tool 被调用，并验证 42%/35% 与正确来源同时出现。
- [ ] citation grader 拒绝不存在、越出 fixture root 或只在正文提名但未形成来源记录的引用。
- [ ] follow-up grader 能识别 session resume 成功、禁止的重新搜索/读取，以及缺失历史 grounding。
- [ ] completed attempt score 为 `Σ(grader score × grader weight) / Σ(weight)`，且只有全部
      required graders pass 时 attempt 才 pass；grader error 产生 null aggregate。
- [ ] 普通 case 三次重复以 2/3 pass，score 取三次算术平均；must-pass case 要求每次 attempt
      都 pass，不能被平均分或多数决掩盖。
- [ ] overall 与 per-tag 使用 case weight 加权；任一必需 attempt 缺失或未完成时 aggregate 为
      null，不缩小分母。
- [ ] validation tag 没有 case 时 aggregator 明确拒绝无效 corpus，而不是输出“无下降”。
- [ ] 每个 grader 使用提交到仓库的固定 trajectory/output fixtures 覆盖 pass、fail 与边界用例。
- [ ] 无 LLM judge、embedding 或远程服务依赖；workspace checks 全绿。

## 备注

- 文风、洞察深度和表达优劣留给 issue 005 的人工 review。
- grader 评价外部行为，不依赖 recorder 内部函数调用或实现细节。
