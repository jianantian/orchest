# 002 — Run Manifest 与 Sensitive Trajectory Recorder

## 背景

`RuntimeEvent` 已提供 model、Tool、approval、sub-agent、budget 和 terminal events，但 Briefing
Desk 目前只把它们渲染到 stdout。Better-Harness 式对比需要可查询 trajectory，同时
`RuntimeEvent` 含 Tool input/output 和生成文本，不能进入默认 tracing 或被误提交到仓库。

## 目标/范围

1. 在 Briefing Desk 应用内定义版本化、sanitized `TrajectoryEvent` schema；recorder 按事件
   到达顺序转换 public `RuntimeEvent`，禁止直接 serde 全量 runtime event。
2. 为每次 run 保存 manifest：label、时间、git commit/dirty、fixture revision、provider/model、
   非秘密 request options、harness/effective-config snapshot path/hash、session seed hashes、
   schema version、case IDs、split 与重复策略。
3. 采用 `evals/runs/<label>/cases/<case-id>/<attempt>/` 的不可覆盖 artifact layout，保存
   `trajectory.jsonl`、`output.md`、`attempt.json` 和 `scores.json`。
4. eval run 必须显式确认敏感录制；runs 目录默认 gitignore，README 说明其中可能包含用户输入、
   Tool payload 和生成文本。
5. 将 provider/network failure、event drop、event stream 提前关闭、缺 terminal event 区分为
   结构化 attempt 状态。
6. 实现 follow-up attempt 隔离：从只读 seed materialize snapshot，每次使用独立临时 SQLite
   store，artifact flush 后清理并记录结果。
7. 每次 run 保存规范化 `harness/snapshot.json` 与 SHA-256；只记录 hash 不算完成。工作区存在
   harness 之外的 dirty path 时在 model call 前拒绝。
8. 在首次 model call 前保存规范化、脱敏的 `EffectiveConfigSnapshot`，覆盖实际 runtime、
   budget/retry/approval、Tool registry schema/metadata、ASR/TTS/vision routing、session mode
   和非秘密环境驱动选项。

## 验收标准

- [ ] 完整成功或失败 attempt 都产生固定四文件；失败时 `scores.json` 明确为 `not_run`，不是
      省略文件。
- [ ] 未显式传敏感录制确认时，eval run 在启动 model call 前失败。
- [ ] recorder 全量丢弃 `ModelStreamChunk`，顶层与嵌套 `Thinking`、
      `ThinkingEnd.signature/provider_details` 都不会出现在 fixture-based 测试输出。
- [ ] retained Values 递归删除 thinking/reasoning/provider detail 字段，并遮盖 API key、
      authorization、cookie、set-cookie、x-api-key、access token 命名变体；recorder 不读取环境。
- [ ] 子 agent/child event 使用同一个递归 sanitizer；`SubAgentStarted.config_summary` 不落盘。
- [ ] `evals/runs/` 被 gitignore；README 明确记录、保留、分享和删除风险。
- [ ] label 已存在时默认拒绝，不覆盖历史 baseline/candidate。
- [ ] `EventsDropped`、无 terminal event 或 event stream 提前关闭产生 `inconclusive`，不得写成
      completed。
- [ ] provider/network failure 保存失败 artifact 并标为 `execution_failure`。
- [ ] run 保存可恢复全部 surface 文本的规范化 snapshot；manifest path/hash 与磁盘字节一致，
      prompt 或任一 Tool description 改变都会改变 hash。
- [ ] run 保存 `effective-config/snapshot.json` 与 SHA-256；它来自解析后的实际配置，不使用
      `AgentConfig` 的不完整 serde 输出，也不包含 harness 文本。
- [ ] manifest 中的 effective-config path/hash 与磁盘规范化字节一致；篡改 snapshot 或 hash
      会在 compare/model-call preflight 被检测。
- [ ] effective config 包含 main/reviewer runtime、budget、retry、approval、Tool execution、
      supervision、各 case Tool 名/schema hash/metadata、capability fake/live/disabled 和 session
      persistence mode。
- [ ] 改变任一有效 runtime/capability/Tool schema 输入都会改变 effective-config hash；只改变
      prompt/Tool description 不会改变该 hash。
- [ ] custom approval、hook、executor、store 等不可直接序列化对象使用稳定配置标签；无法表示
      的启用项、带凭据 URL 或秘密字段在 model call 前失败。
- [ ] manifest 记录 commit、dirty paths 和 session seed hashes；harness 外 dirty path、seed
      hash 不匹配或 API key 泄漏都在 model call 前失败。
- [ ] 改变 API key 环境变量不改变 harness snapshot 或 manifest 非秘密字段，且秘密值不落盘。
- [ ] follow-up 的每个 attempt 使用唯一 session/run ID 与独立 store；三次重复从同一 seed
      hash 开始，前一次 store mutation 不可见，成功/失败后都尝试清理并写入结果。
- [ ] `attempt.json` 明确记录 terminal/status、stop reason、wall latency、各 token 字段、
      resource coverage、session seed/hash、store cleanup 与结构化 error。
- [ ] 使用 scripted/fake model 的离线测试覆盖成功、失败、event drop、无 terminal event 和
      label 冲突；workspace checks 全绿。

## 备注

- 本地 run artifacts 不作为仓库交付物；可提交的是 schema、测试 fixtures 和脱敏 summary。
- 本 issue 不实现 graders 或 baseline/candidate comparison。
