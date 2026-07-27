# 004 — 子代理输出格式契约

## 背景

子代理只有纯文本 output,消费方靠 `strip_code_fences` 这类脆弱逻辑提取(demo
`countdown.rs:276`:只认严格首尾 fence;模型前后加解释文字或输出完整 `<!DOCTYPE>` 都会原样
落盘 → widget 全废)。SDK 优化计划 E2:为 agent-as-tool 增加输出格式契约,由 SDK 层做
提取/校验/纠正。

## 目标/范围

1. **契约声明**: `SubAgentBuilder` 增加输出期望配置(如 `.expect_output(...)`,enum 形如
   `Fenced { lang: Option<String> }` | `Json` | 默认 None 保持现状)。
2. **SDK 层提取/校验**: child 完成后按契约提取——fenced: 从文本中定位首个匹配 fence 块
   (容忍前后解释文字、DOCTYPE 包裹);json: parse 校验(本期"能 parse 即过",schema 校验留后续)。
   提取成功 → `model_output` = 提取结果,details 保留原文。
3. **纠正轮次**: 提取失败 → 经 `AgentRun::resume_with_input`(v0.13)给 child 发一条纠正提示
   (指明期望格式),限 1 次;仍失败 → `Err(ToolError)`(复用 003 的 Err 语义,诊断含原始输出头部)。
   纠正提示与结果可观测(事件或 details)。
4. **demo 采用**: countdown 删 `strip_code_fences`,改声明 `Fenced { lang: Some("html") }`。

## 验收标准

- [x] 格式不符的输出在 SDK 层被提取/纠正,不原样交给消费方
- [x] 纠正轮次限 1 次;纠正后仍失败 → `Err(ToolError)`;纠正提示可观测
- [x] 未声明契约时行为与现状完全一致(回归)
- [x] countdown 改契约声明,`strip_code_fences` 删除,widget 链路测试绿
- [x] 测试:fence 前后带解释文字可提取;无 fence 触发纠正;纠正后成功;纠正后仍失败 → Err
- [x] 五项检查全绿

## 备注

- JSON schema 深度校验留后续(本期 parse 即过)。
- 依赖 003 先落地(Err 语义);依赖 v0.13 `resume_with_input`。
- **实现记录(机制偏差)**: 纠正轮次未用 `resume_with_input`——它面向跨进程 session
  恢复(需 `SessionSnapshot` + `session_store`),tool 内部拿不到 child 快照。改为
  **自包含纠正提示的第二次 fresh child run**(提示含原始请求 + 违规输出头部 500 字符),
  限 1 次,usage 跨两次累加;契约配置为 `SubAgentBuilder::expect_output(SubAgentOutputExpect)`,
  违反经纠正确认后 `Err(ToolError, code=SUB_AGENT_OUTPUT_CONTRACT_VIOLATION)`;纠正触发发
  `RuntimeWarning` 事件(未新增事件变体,Py/Node wire 无影响)。demo countdown.md 模板同步
  要求单 ```html 围栏块(否则契约与模板指令冲突,每次都要走纠正轮)。
