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
2. 通过 `RuntimeEvent` 在应用层生成 opt-in、敏感但经过明确净化的 trajectory，并为每次运行
   保存可恢复候选文本的 harness snapshot、effective runtime configuration snapshot 与
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

每次 run 必须保存 `harness/snapshot.json`。snapshot 是按稳定 `surface_id` 排序的数组，包含
每个 prompt/description 的准确文本；文本先把 CRLF 规范化为 LF，但不 trim，随后用固定字段顺序
序列化为 UTF-8 JSON。`harness/snapshot.sha256` 和 manifest 中的 `harness_snapshot.sha256`
必须对同一组规范化字节计算。仅有 hash 不算可复现：给定 manifest 的 git commit 和 snapshot，
必须能恢复该次运行使用的全部可编辑 surface。

eval run 允许工作区在声明的 harness 文件内 dirty，但必须记录 dirty paths；若存在 harness
之外的 dirty path，必须在 model call 前拒绝运行。这样 baseline/candidate 的应用逻辑来自同一
commit，候选差异则由本地 snapshot 完整保存。

### Eval cases

案例使用应用内、可版本控制的结构化文件。每个 case 至少包含：

- 稳定 `case_id`
- `scenario_family`
- 一个或多个行为标签
- `split`: `optimization`、`validation` 或 `scorecard`
- 输入问题及运行模式：初始 run 或 session follow-up
- follow-up 使用的只读 session seed ID 与 hash
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

所有注册的行为标签都属于 validation gate：4 个 validation case 可以携带多个标签，但 corpus
validator 必须证明每个注册标签至少被一个 validation case 覆盖。缺少 validation 覆盖的标签会
使 corpus 无效，不能把该标签默认为“未下降”。

### Follow-up session 生命周期

follow-up case 不通过 live model 临时生成前置对话，而是引用仓库内版本化、合成的
`session seed`。seed 只包含此前消息、step 与 budget usage，不包含可变的 session ID、run ID、
store path 或 harness config。其生命周期固定为：

1. runner 在任何 model call 前校验 seed，并计算规范化内容 hash；
2. 每个 attempt 从同一只读 seed 独立 materialize 一个 `SessionSnapshot`，注入本次 harness
   config，并分配唯一 session ID/run ID；
3. 每个 attempt 使用独立的临时 SQLite store；同一 case 的三次重复不得复用 mutable store；
4. baseline 与 candidate 必须引用同一个 seed ID/hash，messages、step 与初始 budget 完全相同；
   唯一允许不同的是被测 harness config 和新生成的运行标识；
5. `resume_with_input` 的计时从 seed materialize 完成后开始；
6. terminal/failure artifact flush 后删除临时 store；清理失败写入 `attempt.json`，不得静默忽略。

因此重复运行都从相同的逻辑只读 snapshot 出发，同时不会共享可能被前一次 attempt 污染的
session/store。

### Run manifest

每次 eval run 保存 `manifest.json`，至少包含：

- run label 与时间
- Orchest git commit、dirty 状态和 dirty paths
- Briefing Desk fixture revision
- provider、model 和非秘密 request options
- `harness/snapshot.json` 的相对路径、SHA-256 和各 surface hash
- `effective-config/snapshot.json` 的相对路径、SHA-256 和 schema version
- 所有 follow-up session seed 的 ID 与内容 hash
- runtime/package schema version
- split、case IDs 与 repetition policy

manifest 不得包含 API key、authorization、cookie 或 reasoning 正文。

### Effective runtime configuration

只保存 harness 文本仍不足以复现运行。每次 run 必须在环境变量解析完成、`AgentConfig` 和
`ToolRegistry` 构造完成、第一次 model call 之前，保存规范化、脱敏的
`effective-config/snapshot.json`。它是应用层显式定义的 `EffectiveConfigSnapshot`，不能直接
序列化 `AgentConfig`：`retry_policy`、hooks、session store、custom approval 等字段带有
`serde(skip)`，直接序列化会产生不完整配置。

snapshot 使用稳定字段顺序和 UTF-8 JSON，至少覆盖：

- main/reviewer agent 的 model identity、非秘密 request options、runtime max steps、
  allowed tools、tool search、compaction、run depth、repeated-failure threshold 与 supervision；
- budget limits、retry 次数/backoff/jitter、approval mode、是否存在 custom approval、Tool
  execution policy；
- 每个 case profile 的 Tool registry：稳定排序的 Tool 名、input/output schema SHA-256、
  side-effect/approval/execution-mode/parallelism/timeout/max-output/source metadata；Tool
  description 只记录 `surface_id`，正文和 hash 留在 harness snapshot，避免把合法候选差异写进
  effective config；
- ASR、TTS、vision 的 `fake` / `live` / `disabled` 选择，以及 live 时的 provider、model 和
  非秘密 endpoint；fresh/follow-up 的 session persistence mode；
- 所有影响行为的非秘密环境驱动选项解析后的值，而不是仅记录环境变量名。

trait object、closure 或 store 实例不能写地址/`Debug` 文本；应用必须把它们映射为稳定的配置
标签。无法完整表示的已启用配置使 preflight 失败。API key、authorization、cookie、带凭据的
URL userinfo/query 和临时 session/run/store 标识不得进入 snapshot。

`effective-config/snapshot.sha256` 与 manifest hash 对规范化 snapshot 字节计算。baseline 与
candidate 必须有相同 effective-config hash；compare 必须先从磁盘重新计算两边的 hash，再比较
snapshot，并对不一致给出字段级 diff。harness snapshot 是两者唯一允许不同的执行输入。

### Sensitive trajectory

`trajectory.jsonl` 不是 `RuntimeEvent` 的直接 serde 输出，而是 schema-versioned 的应用级
`TrajectoryEvent`。每行只包含 `schema_version`、严格递增的 `sequence`、attempt 起点后的
`elapsed_ms`、run/child 关系、`kind` 与净化后的 `data`。净化策略是 allowlist：

| `RuntimeEvent` 来源 | 处理 | 规则 |
|---------------------|------|------|
| `ModelStreamChunk` 全部变体 | 丢弃 | 包括 `Thinking`、`ThinkingEnd.signature/provider_details`、文本/tool args delta、音频与 extension；最终文本和 Tool 调用由 canonical runtime events 记录 |
| model call start/completed/retry | 保留/脱敏 | 保留 step、usage、option adjustments、attempt、delay；错误文本做 secret-pattern 脱敏 |
| Tool start/update/completed/failed/retry、async Tool、batch | 保留/脱敏 | 保留 Tool 名、顺序、状态、时长；input/output/partial 递归删除 `thinking`、`reasoning`、`signature`、`provider_details`，并遮盖 secret-key 字段 |
| approval | 保留/脱敏 | 保留 Tool 名、call ID、context 与净化后的参数，不保存未净化 `ToolCall` |
| skill、budget、warning、compaction、agent update、restart | 保留/脱敏 | 只保留 grader/诊断所需字段；自由文本做 secret-pattern 脱敏 |
| sub-agent/child wrappers | 递归处理 | 保留 parent/child ID 与 depth；嵌套事件重新走同一 allowlist，嵌套 thinking 仍必须丢弃 |
| `SubAgentStarted.config_summary` | 丢弃 payload | 只保留 parent/child ID，避免 config 携带 prompt |
| terminal events | 保留/脱敏 | 保留 stop reason/status 与净化后的 final output/error |
| `EventsDropped` | 保留并升级状态 | artifact 保留 count，但 attempt 必须是 `inconclusive` |

字段名匹配先转小写并去除 `_`、`-` 等分隔符。secret-key 字段至少包括命名变体的 `api_key`、
`authorization`、`cookie`、`set_cookie`、`x_api_key` 和 `access_token`。recorder 不读取进程
环境；净化后的 Tool payload、用户输入和最终报告仍可能敏感，因此 trajectory 仍按敏感产物
管理：

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
├── harness/
│   ├── snapshot.json
│   └── snapshot.sha256
├── effective-config/
│   ├── snapshot.json
│   └── snapshot.sha256
├── results.json
├── summary.md
└── cases/<case-id>/<attempt>/
    ├── trajectory.jsonl
    ├── output.md
    ├── attempt.json
    └── scores.json
```

四个 attempt 文件始终存在。`attempt.json` 固定记录 status、起止时间、wall latency、terminal
kind、stop reason、各 token 字段、resource coverage、session seed/hash、临时 store 清理结果
和结构化 error；失败或 inconclusive 时 `output.md` 可为空，`scores.json` 明确写
`grader_status: not_run` 和 null aggregates，不能省略文件造成歧义。

### Deterministic graders

第一版只评价 trajectory 和产物中可确定观察的行为：

- Tool 是否被选择或禁止；
- Tool 调用顺序是否满足约束；
- 文本、音频、图片证据是否覆盖；
- 42% 与 35% 的冲突是否同时出现并正确归因；
- 报告是否含结论、证据、风险和来源；
- 引用的 fixture 是否真实存在；
- follow-up 是否利用已有 session，而非重新执行完整材料流程。

每个 grader 输出 `passed`、0–100 `score` 与正权重。一个 completed attempt 的 case score 为
grader 分数的加权算术平均；只有全部 required graders 都 `passed=true` 时 attempt 才 pass。
grader error 使 attempt grading 状态为 `inconclusive`，不产生可用于比较的 case score。

三次重复时，非 must-pass case 以至少 2/3 attempt pass 为 case pass，case score 取三次
attempt score 的算术平均；must-pass case 要求 3/3 attempt 均绝对 pass。split overall score
按 case weight 对 case score 加权；per-tag score 对携带该 tag 的 validation cases 使用同一
case weight 加权。若任一必需 attempt 未 completed/grading completed，该 case/split aggregate
为 null，compare 直接失败，不缩小分母。

不使用 LLM judge。无法可靠规则化的文风、洞察质量和表达优劣留给人工 review。

### Candidate comparison

baseline 与 candidate 只有在 source git commit、effective-config hash、fixture revision、
session seed hashes、case set、split 和 repetition policy 一致时才可比较。两次 run 都不得
有 harness 外 dirty path；harness snapshot 是预期差异，不要求 hash 相同。模型、应用逻辑或
effective runtime configuration 变更属于新实验，不能伪装成 harness candidate。

baseline 首先必须有效：参与比较的全部 must-pass cases 在每次 repetition 中都绝对通过。只要
baseline 有一个 must-pass attempt 失败，比较结果为 `invalid_baseline`，必须记录失败 case，
且不能计算 candidate eligibility。不能用“candidate 没比 baseline 更差”掩盖 baseline 已失败。

有效 baseline 下，candidate 进入人工审核必须同时满足：

1. Briefing Desk 与相关 workspace 测试通过；
2. candidate 的全部 must-pass cases 在每次 repetition 中绝对通过，不只做相对 baseline 判断；
3. validation 加权总分相对 baseline 至少提高 5 分（100 分制）；
4. 任一行为标签的 validation 分数不得下降；
5. validation 每个 case 运行 3 次，并使用上一节固定的 grader/case/split 聚合公式；
6. validation attempt 的平均 gate total tokens 不超过 baseline 的 115%；
7. validation attempt 的中位 wall latency 不超过 baseline 的 130%；
8. 所有参与比较的 case 均为 completed，不能用 inconclusive 抵消失败。

资源口径固定如下：

- 每个 attempt 分别求和所有可观察 model call 的 `input_tokens`、`output_tokens`、
  `reasoning_tokens`、`audio_input_tokens`、`image_input_tokens` 与 `video_input_tokens`；
- `gate_total_tokens = input_tokens + output_tokens + audio_input_tokens +
  image_input_tokens + video_input_tokens`；`reasoning_tokens` 是 output usage 的诊断性细分，
  协议未保证它与 `output_tokens` 互斥，因此单独报告且不再次加入；
- `cache_read_tokens`、`cache_write_tokens` 和 `details` 同样单独报告但不加入 gate total，
  避免重复计数；
- normal model calls、递归 child events 和 Briefing Desk 内部 vision call 都必须上报 usage；
  任一已知内部 model call 缺 usage 时 `resource_coverage=incomplete`，compare 失败；
- run 的 validation mean tokens 是所有 validation attempts 的 `gate_total_tokens` 等权平均，
  不是先按 case 求均值，也不使用 case weight；完整性 gate 保证分母固定；
- wall latency 使用 monotonic clock，从紧邻 `AgentRun::start` 或 `resume_with_input` 前开始，
  到 terminal event 已接收且 `RunHandle::wait` 已返回两者都满足时结束；corpus 校验、seed
  materialize、grader 与 artifact I/O 不计入，run 内的自动 approval 处理计入；
- 失败/inconclusive attempt 仍记录 latency，但不进入 median；由于完整性 gate 失败，不能靠
  排除慢失败来取得 eligibility。偶数样本的 median 是排序后中间两项的算术平均。

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
- follow-up seed hash 不匹配、attempt store 无法隔离或清理失败：保存 attempt，并使 run
  `inconclusive`。
- baseline must-pass 失败：compare 返回 `invalid_baseline`，不评价 candidate eligibility。
- grader error、缺 token usage 或 validation tag 无覆盖：compare/corpus 校验失败，不缩小分母。
- provider 未提供 pricing：cost 标为 unknown，token gate 仍执行。
- baseline/candidate manifest 不可比：compare 命令失败并列出不一致字段。
- effective config 有无法稳定表示的启用项、秘密字段或 snapshot/hash 不一致：任何 model call
  前失败。
- 单个 candidate 失败不得覆盖已有 baseline 或其他 candidate 目录；label 冲突默认拒绝。

## Issue 分解

| Issue | 标题 | 交付物 |
|-------|------|--------|
| 001 | Harness surfaces 与 Eval Corpus | 集中可编辑面、case schema、18 个 cases、split 校验 |
| 002 | Run Manifest 与 Sensitive Trajectory Recorder | opt-in recorder、harness/effective-config snapshots、manifest、失败状态 |
| 003 | Deterministic Graders | Tool/order/modality/conflict/report/citation/follow-up graders |
| 004 | Eval Runner 与 Candidate Comparison | CLI、重复运行、gate、compare report、scorecard 确认 |
| 005 | 人工 Harness 实验与验证报告 | live baseline/candidates/scorecard、脱敏验证报告与结论 |

依赖顺序：`001 → 002 → 003 → 004 → 005`。

## 整体验收

- [x] 五个 issue 的 acceptance criteria 全部通过。
- [x] 18 个 cases 可在 model call 前完成 schema、split、scenario-family 和 fixture 校验。
- [x] CI 使用 scripted/fake model 覆盖 recorder、graders、runner、compare 和失败路径。
- [x] recorder 测试证明顶层及嵌套 `Thinking`/`ThinkingEnd.provider_details` 不会进入 artifact。
- [x] effective config snapshot 覆盖 runtime、Tool registry、capability routing 与 session
      mode；baseline/candidate compare 要求 hash 相同。
- [x] follow-up 三次重复与 baseline/candidate 都从同一个只读 seed hash 开始，且 attempt
      store 互不污染。
- [x] 至少一次使用同一个 live chat model 的 baseline/candidate 对比被完整记录。
- [x] 人工最多提出并运行三个候选；每个候选只修改 harness surfaces。
- [x] 有 eligible candidate 时才运行 sealed scorecard；没有 eligible candidate 也可完成 iteration，
      但报告必须如实记录。
- [x] `docs/review/v0_16_eval_lab.md` 记录模型、日期、假设、修改、分数、成本、回归和最终决定，
      且不包含秘密或完整敏感 trajectory。
- [x] `orchest`、`orchest-protocol`、provider crates 与 bindings 无改动。
- [x] `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、
      `cargo fmt --check`、`bash scripts/lint-check.sh` 全绿。

## 依赖与发布关系

- 依赖已完成的 v0.10 Briefing Desk 和 v0.15 runtime/tool 语义。
- 不依赖 v0.11 Demo B。
- 不阻塞 v1.0；试点发现的 runtime gap 只能记录为独立 finding，不能在本 iteration 顺手修改
  core。
