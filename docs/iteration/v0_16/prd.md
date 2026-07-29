# v0.16 PRD: Briefing Desk Eval Lab

## 背景

Briefing Desk 在 v0.10 中证明了 Orchest 的 runtime、Tool、approval、session、Agent-as-Tool、
ASR、TTS 与多模态图片输入可以组合成一个完整产品形态。它也暴露了一个后续问题：当 system
prompt 或 Tool description 改动后，项目没有一套可复现的方法判断 agent 是真的变好，还是只在
单次运行中随机表现较好。

Better-Harness、Meta-Harness 一类工作把 eval 视为 harness engineering 的反馈信号，但
Orchest 不应因此把数据集管理、自动优化器或敏感 trajectory 存储塞进最小 runtime core。
v0.16 以 Briefing Desk 为唯一试点，在应用层建立一个小型、人工驱动的垂直闭环：

```text
固定 eval cases
      ↓
运行 baseline
      ↓
人工修改 system prompt / Tool descriptions
      ↓
运行 candidate
      ↓
自动评分、回归检查与成本比较
      ↓
生成人工可审阅的接受/拒绝报告
```

本迭代是 v1.0 前的独立、非阻塞实验。它不改变 v1.0 的发布门槛，也不依赖仍在规划中的
v0.11 Demo B。

## 目标

1. 为 Briefing Desk 建立 18 个高信号 eval cases，覆盖工具选择、工具组合、多模态证据、
   冲突处理、报告结构、引用质量和 follow-up grounding。
2. 通过 `RuntimeEvent` 在应用层记录 opt-in、敏感的 trajectory，并为每次运行保存可复现的
   manifest。
3. 用确定性 graders 自动评价可观察行为，不在第一版引入 LLM judge。
4. 自动比较 baseline 与 candidate，执行 must-pass、分类回归、分数、token 和 latency gate，
   但把最终接受权留给人。
5. 使用同一个 live chat model 完成至少一次人工 harness 改进实验，产出公开、脱敏的验证报告。

## 非目标

- 不实现 outer agent，不自动提出或写入 harness 修改。
- 不创建通用 eval crate、远程服务或 LangSmith 集成。
- 不修改 `orchest`、`orchest-protocol`、`orchest-provider-*` 或 bindings 的公开 API。
- 不把 trajectory payload 写入默认 `tracing` 或 metrics。
- 不为了试点给 Briefing Desk 人为增加 Skill；当前可编辑面只有 system prompt 与 Tool
  descriptions。
- 不用 prompt/eval 绕过 runtime correctness 问题。approval、resume、retry、truncation 等
  runtime 语义继续由确定性 Rust 测试保护。
- 不宣称跨领域泛化。18 个案例均基于现有 Loom fixture corpus；scorecard 只证明本试点的
  未见场景表现。
- 不要求 live ASR/TTS provider 验证；本迭代验证 agent 行为，ASR/TTS 可继续使用 public fake。

## 产品形态

在 `examples/demo/briefing-desk` 内新增 **Eval Lab**。用户先对当前 harness 运行 baseline，
人工修改集中管理的 system prompt 或 Tool descriptions，再运行 candidate。Eval Lab 保存
每个 case 的 trajectory、输出、分数与 run manifest，并生成 baseline/candidate 对比报告。

建议命令形态：

```bash
briefing-desk eval run \
  --label baseline \
  --split optimization,validation \
  --record-sensitive

# 人工修改 Briefing Desk harness surfaces

briefing-desk eval run \
  --label candidate-1 \
  --split optimization,validation \
  --record-sensitive

briefing-desk eval compare baseline candidate-1

briefing-desk eval run \
  --label final \
  --split scorecard \
  --record-sensitive \
  --confirm-sealed
```

命令名称和参数可以在实现时按现有 clap 结构微调，但必须保持这些语义：显式 label、显式 split、
显式敏感录制确认，以及 scorecard 的额外确认。

## 架构边界

### Harness surfaces

Briefing Desk 当前散落在 `app.rs`、`tools.rs` 和 `media.rs` 的 system prompt 与 Tool
descriptions 集中到应用内 `harness` 模块。业务逻辑、Tool schema、Tool execute 实现和 runtime
配置不属于可编辑 surface。

Harness snapshot 必须能被 manifest 完整描述或 hash；baseline/candidate 对比不得依赖“当前
工作区大概是什么状态”。

### Eval cases

案例使用应用内、可版本控制的结构化文件。每个 case 至少包含：

- 稳定 `case_id`
- `scenario_family`
- 一个或多个行为标签
- `split`: `optimization`、`validation` 或 `scorecard`
- 输入问题及运行模式：初始 run 或 session follow-up
- must-pass 标记与 grader 权重
- 所需/禁止 Tool、必要的顺序约束
- 预期事实、冲突、报告 section 与 fixture 引用

首轮固定为 18 个 case：

| Split | 数量 | 用途 |
|-------|------|------|
| optimization | 10 | 人可以查看失败细节并据此修改 |
| validation | 4 | 比较候选；每个 case 运行 3 次 |
| scorecard | 4 | 最终候选才运行；每个 case 运行 3 次 |

同一问题的改写或同一 `scenario_family` 不得跨 split，避免明显的数据泄漏。

### Run manifest

每次 eval run 保存 `manifest.json`，至少包含：

- run label 与时间
- Orchest git commit 和 dirty 状态
- Briefing Desk fixture revision
- provider、model 和非秘密 request options
- system prompt 与 Tool description 的内容 hash
- runtime/package schema version
- split、case IDs 与 repetition policy

manifest 不得包含 API key、authorization、cookie 或 reasoning 正文。

### Sensitive trajectory

`trajectory.jsonl` 是按顺序序列化的应用级事件记录，来源是 public `RuntimeEvent`。它可能包含
Tool input/output、模型生成文本和最终报告，因此属于敏感产物：

- 只有显式 `--record-sensitive` 才能运行 eval；
- `evals/runs/` 默认加入 `.gitignore`；
- 不复用默认 tracing/metrics 作为 payload 存储；
- 记录前后均不得主动收集 API key、authorization 或 hidden reasoning；
- 出现 `RuntimeEvent::EventsDropped`、事件流提前关闭或无 terminal event 时，case 标为
  `inconclusive`，不得继续计入行为分数。

每个 run 的本地产物：

```text
evals/runs/<label>/
├── manifest.json
├── results.json
├── summary.md
└── cases/<case-id>/<attempt>/
    ├── trajectory.jsonl
    ├── output.md
    └── scores.json
```

### Deterministic graders

第一版只评价 trajectory 和产物中可确定观察的行为：

- Tool 是否被选择或禁止；
- Tool 调用顺序是否满足约束；
- 文本、音频、图片证据是否覆盖；
- 42% 与 35% 的冲突是否同时出现并正确归因；
- 报告是否含结论、证据、风险和来源；
- 引用的 fixture 是否真实存在；
- follow-up 是否利用已有 session，而非重新执行完整材料流程。

不使用 LLM judge。无法可靠规则化的文风、洞察质量和表达优劣留给人工 review。

### Candidate comparison

baseline 与 candidate 只有在 provider、model、request options、fixture revision、case set 和
repetition policy 一致时才可比较。模型变更属于新实验，不能伪装成 harness candidate。

candidate 进入人工审核必须同时满足：

1. Briefing Desk 与相关 workspace 测试通过；
2. must-pass cases 零回归；
3. validation 加权总分相对 baseline 至少提高 5 分（100 分制）；
4. 任一行为标签的 validation 分数不得下降；
5. validation 每个 case 运行 3 次，以多数结果决定 pass/fail，连续值使用三次均值；
6. 平均 token 不超过 baseline 的 115%；
7. 中位 latency 不超过 baseline 的 130%；
8. 所有参与比较的 case 均为 completed，不能用 inconclusive 抵消失败。

比较器只输出 `eligible_for_review` 或不满足条件的具体原因，不自动写入或接受候选。

### Sealed scorecard

scorecard 只有候选被人工选中后才能通过 `--confirm-sealed` 运行。若人查看 scorecard 的逐 case
失败细节并据此继续修改，该 scorecard 即失封；验证报告必须记录失封，并在下一轮补充或轮换
案例。v0.16 不实现硬安全隔离，sealed 是明确的实验流程约束。

## 错误处理

- case schema、fixture 引用或 split 约束错误：任何 model call 前失败。
- live chat model 未配置：eval run 响亮失败，不像普通 smoke test 一样跳过。
- provider/network 失败：保存失败 attempt，case 标为 execution failure，不计行为分。
- trajectory 丢事件或缺 terminal event：case 标为 inconclusive。
- provider 未提供 pricing：cost 标为 unknown，token gate 仍执行。
- baseline/candidate manifest 不可比：compare 命令失败并列出不一致字段。
- 单个 candidate 失败不得覆盖已有 baseline 或其他 candidate 目录；label 冲突默认拒绝。

## Issue 分解

| Issue | 标题 | 交付物 |
|-------|------|--------|
| 001 | Harness surfaces 与 Eval Corpus | 集中可编辑面、case schema、18 个 cases、split 校验 |
| 002 | Run Manifest 与 Sensitive Trajectory Recorder | opt-in recorder、本地 artifact layout、manifest、失败状态 |
| 003 | Deterministic Graders | Tool/order/modality/conflict/report/citation/follow-up graders |
| 004 | Eval Runner 与 Candidate Comparison | CLI、重复运行、gate、compare report、scorecard 确认 |
| 005 | 人工 Harness 实验与验证报告 | live baseline/candidates/scorecard、脱敏验证报告与结论 |

依赖顺序：`001 → 002 → 003 → 004 → 005`。

## 整体验收

- [ ] 五个 issue 的 acceptance criteria 全部通过。
- [ ] 18 个 cases 可在 model call 前完成 schema、split、scenario-family 和 fixture 校验。
- [ ] CI 使用 scripted/fake model 覆盖 recorder、graders、runner、compare 和失败路径。
- [ ] 至少一次使用同一个 live chat model 的 baseline/candidate 对比被完整记录。
- [ ] 人工最多提出并运行三个候选；每个候选只修改 harness surfaces。
- [ ] 有 eligible candidate 时才运行 sealed scorecard；没有 eligible candidate 也可完成 iteration，
      但报告必须如实记录。
- [ ] `docs/review/v0_16_eval_lab.md` 记录模型、日期、假设、修改、分数、成本、回归和最终决定，
      且不包含秘密或完整敏感 trajectory。
- [ ] `orchest`、`orchest-protocol`、provider crates 与 bindings 无改动。
- [ ] `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、
      `cargo fmt --check`、`bash scripts/lint-check.sh` 全绿。

## 依赖与发布关系

- 依赖已完成的 v0.10 Briefing Desk 和 v0.15 runtime/tool 语义。
- 不依赖 v0.11 Demo B。
- 不阻塞 v1.0；试点发现的 runtime gap 只能记录为独立 finding，不能在本 iteration 顺手修改
  core。

