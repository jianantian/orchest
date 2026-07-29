# 002 — Run Manifest 与 Sensitive Trajectory Recorder

## 背景

`RuntimeEvent` 已提供 model、Tool、approval、sub-agent、budget 和 terminal events，但 Briefing
Desk 目前只把它们渲染到 stdout。Better-Harness 式对比需要可查询 trajectory，同时
`RuntimeEvent` 含 Tool input/output 和生成文本，不能进入默认 tracing 或被误提交到仓库。

## 目标/范围

1. 在 Briefing Desk 应用内实现 eval recorder，按事件到达顺序把 public `RuntimeEvent`
   序列化为 JSONL；不修改 runtime event contract。
2. 为每次 run 保存 manifest：label、时间、git commit/dirty、fixture revision、provider/model、
   非秘密 request options、harness content hashes、schema version、case IDs、split 与重复策略。
3. 采用 `evals/runs/<label>/cases/<case-id>/<attempt>/` 的不可覆盖 artifact layout，保存
   trajectory、output、attempt status 与基础 token/latency。
4. eval run 必须显式确认敏感录制；runs 目录默认 gitignore，README 说明其中可能包含用户输入、
   Tool payload 和生成文本。
5. 将 provider/network failure、event drop、event stream 提前关闭、缺 terminal event 区分为
   结构化 attempt 状态。

## 验收标准

- [ ] 完整成功 run 产生有序 `trajectory.jsonl`、`output.md`、attempt metadata 与
      `manifest.json`。
- [ ] 未显式传敏感录制确认时，eval run 在启动 model call 前失败。
- [ ] recorder 不主动采集 API key、authorization、cookie 或 hidden reasoning；相应字段不出现
      在 fixture-based recorder 测试输出。
- [ ] `evals/runs/` 被 gitignore；README 明确记录、保留、分享和删除风险。
- [ ] label 已存在时默认拒绝，不覆盖历史 baseline/candidate。
- [ ] `EventsDropped`、无 terminal event 或 event stream 提前关闭产生 `inconclusive`，不得写成
      completed。
- [ ] provider/network failure 保存失败 artifact 并标为 `execution_failure`。
- [ ] manifest 的 harness hash 会在 prompt 或任一 Tool description 改变时变化；API key 改变
      不影响也不进入 manifest。
- [ ] 使用 scripted/fake model 的离线测试覆盖成功、失败、event drop、无 terminal event 和
      label 冲突；workspace checks 全绿。

## 备注

- 本地 run artifacts 不作为仓库交付物；可提交的是 schema、测试 fixtures 和脱敏 summary。
- 本 issue 不实现 graders 或 baseline/candidate comparison。

