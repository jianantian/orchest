# 001 — Harness surfaces 与 Eval Corpus

## 背景

Briefing Desk 的 system prompt 位于 `app.rs`，Tool descriptions 分布在 `tools.rs` 与
`media.rs`。如果 eval candidate 可以随意修改应用代码，就无法区分 harness tuning 与业务逻辑/
runtime 修改。本 issue 先锁定唯一可编辑面，再建立可在 model call 前验证的 eval corpus。

## 目标/范围

1. 新增应用内 `harness` 模块，集中保存主 agent system prompt、reviewer prompt 及所有 Briefing
   Desk Tool descriptions；现有 Tool 实现只引用这些定义，不复制文本。
2. 明确 candidate 只允许修改该模块中的 prompt/description 内容；Tool schema、执行逻辑、
   runtime 配置和 fixtures 不是 candidate surface。
3. 定义版本化 eval case schema，覆盖 case ID、scenario family、行为标签、split、运行模式、
   follow-up session seed、must-pass、权重、Tool 约束、事实/冲突、报告结构和 fixture 引用。
4. 编写 18 个案例：10 optimization、4 validation、4 scorecard；同一 scenario family 不跨
   split。
5. 提供 corpus validator，在任何 model call 前检查 schema、ID 唯一性、split 数量、
   scenario-family 隔离、validation tag 覆盖、fixture 与 session seed 引用存在性。
6. 为 follow-up case 提供版本化的合成 session seed；seed 只含 messages、step 和 budget，
   不含 session/run ID、store path 或 harness config。

## 验收标准

- [ ] system prompt、reviewer prompt 与 Tool descriptions 只在 `harness` 模块定义一次，现有
      `run`/`resume` 行为不变。
- [ ] 18 个 case 全部有稳定 ID、scenario family、至少一个行为标签和明确 split。
- [ ] split 数量严格为 optimization 10、validation 4、scorecard 4。
- [ ] 同一 scenario family 不跨 split；validator 对违规配置在 model call 前报错。
- [ ] case 引用不存在的 fixture、重复 ID、未知标签、非法权重或空预期时，validator 给出包含
      case ID 和字段名的错误。
- [ ] case 覆盖 `tool_selection`、`tool_chaining`、`modality_coverage`、
      `conflict_reconciliation`、`report_structure`、`citation_quality`、
      `followup_grounding` 七类行为。
- [ ] 七个行为标签都至少出现在一个 validation case；缺少覆盖时 validator 在 model call 前
      失败，不能把 absent tag 当作未下降。
- [ ] follow-up case 引用的 session seed 可独立校验和 hash；缺失、hash 不符、包含 mutable
      session/run ID 或 harness config 时拒绝 corpus。
- [ ] 测试直接解析提交到仓库的完整 corpus，并验证全部约束；另有每类非法配置的负例测试。
- [ ] `cargo test -p briefing-desk-demo`、workspace test/clippy/fmt/lint-check 全绿。

## 备注

- 案例全部基于现有 Loom corpus；不得在报告中宣称跨领域泛化。
- 本 issue 只建立 surface 与数据合同，不运行真实模型，不记录 trajectory。
